# LayerFS v0.1.3 Phase 1 initial baseline

Evidence: **REVISE**. Product: **NOT_ESTABLISHED**. Phase 1 terminal gate: **NO_GO**.

Sealed source: `e32469e975e8e185ca525b02bb71d70bafa4e865`. Report generator: `ff805b56e3c46c7204cdccc0ff3b1ffc15d6654be347ce5f9da0981f1674befc`.

| Coverage | Count |
| --- | ---: |
| planned_new_cases | 130 |
| planned_initial_sample_slots | 390 |
| executed_initial_sample_slots | 348 |
| planned_new_verification_slots | 390 |
| executed_new_verification_slots | 37 |
| planned_reliability_subcases | 28 |
| executed_reliability_subcases | 1 |
| planned_capped_performance_slots | 25 |
| executed_capped_performance_slots | 25 |
| planned_capped_verifiers | 5 |
| executed_capped_verifiers | 0 |
| missing_slots | 344 |
| invalid_slots | 0 |
| unexecuted_slots | 0 |
| unknown_product_outcomes | 0 |
| product_failed_outcomes | 0 |
| original_new_cases | 130 |
| original_new_performance_slots | 390 |
| suppressed_new_cases | 14 |
| suppressed_new_performance_slots | 42 |
| active_new_cases | 116 |
| active_new_performance_slots | 348 |
| active_new_verification_slots | 348 |
| original_capped_cases | 5 |
| original_capped_performance_slots | 25 |
| suppressed_capped_cases | 0 |
| suppressed_capped_performance_slots | 0 |
| active_capped_performance_slots | 25 |
| active_capped_verification_slots | 5 |
| suppressed_associated_verification_slots | 42 |
| active_required_slots | 755 |
| suppressed_prescribed_slots | 84 |

## Fast iteration profile

Fast results remain separate from fully verified evidence and never fill required Phase1 slots.

- `tiny-stat-1` seed 1: fast_iteration_verified; full gate contribution: none; evidence `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-1-s1-fast-verify-4173c0ddecb8`.

## Phase1 runtime scope suppressions

The original inventory remains visible. Suppressed exact IDs and their associated verification are outside active Phase1 coverage; suppression is neither PASS nor FAIL nor unimplemented work. Their raw historical outcomes are preserved. All active correctness/resource/cleanup gates still apply. Git remains wired with all four execution subsets suppressed.

| Case | Phase1 scope | Reason |
| --- | --- | --- |
| `dedup-history-distributed-500` | suppressed_phase1_time_budget | Explicit user-authorized initial14 Phase1 runtime exclusion; no confirmation run required |
| `dedup-history-unrelated-100` | suppressed_phase1_time_budget | Explicit user-authorized initial14 Phase1 runtime exclusion; no confirmation run required |
| `dedup-history-unrelated-500` | suppressed_phase1_time_budget | Explicit user-authorized initial14 Phase1 runtime exclusion; no confirmation run required |
| `directory-content-scan-500` | suppressed_phase1_time_budget | Explicit user-authorized initial14 Phase1 runtime exclusion; no confirmation run required |
| `git-tool-1` | suppressed_phase1_time_budget | Explicit user-authorized initial14 Phase1 runtime exclusion; no confirmation run required |
| `git-tool-10` | suppressed_phase1_time_budget | Explicit user-authorized initial14 Phase1 runtime exclusion; no confirmation run required |
| `git-tool-100` | suppressed_phase1_time_budget | Explicit user-authorized initial14 Phase1 runtime exclusion; no confirmation run required |
| `git-tool-500` | suppressed_phase1_time_budget | Explicit user-authorized initial14 Phase1 runtime exclusion; no confirmation run required |
| `namespace-subtree-relocate-delete-500` | suppressed_phase1_time_budget | Explicit user-authorized initial14 Phase1 runtime exclusion; no confirmation run required |
| `tiny-bulk-create-100` | suppressed_phase1_time_budget | Explicit user-authorized initial14 Phase1 runtime exclusion; no confirmation run required |
| `tiny-bulk-create-500` | suppressed_phase1_time_budget | Explicit user-authorized initial14 Phase1 runtime exclusion; no confirmation run required |
| `tiny-bulk-delete-500` | suppressed_phase1_time_budget | Explicit user-authorized initial14 Phase1 runtime exclusion; no confirmation run required |
| `workspace-dense-rewrite-100` | suppressed_phase1_time_budget | Explicit user-authorized initial14 Phase1 runtime exclusion; no confirmation run required |
| `workspace-dense-rewrite-500` | suppressed_phase1_time_budget | Explicit user-authorized initial14 Phase1 runtime exclusion; no confirmation run required |

## Retained original and corrected source arms

Raw outcomes keep their actual producing identities and pass/fail statuses. Every unrequested SQL-history performance recording is diagnostic-only: source labelling does not repair its contaminated timers or memory observations.

| Arm | Source / identity group | Image | Raw performance outcomes | Raw pass | Raw fail | Invalidated observations | SQL history scope |
| --- | --- | --- | ---: | ---: | ---: | ---: | --- |
| corrected | `6c54f8d74a8f07867c6b658da674603c4be6a7c3` / `2f70f944e3789483` | `sha256:487b3d834fb8f3f5c566ab90467610e2955774a58aafc6aa3df763c84c5c312a` | 413 | 413 | 0 | 0 | explicit-opt-in; default capture disabled |
| corrected | `7948df2de269e5ffd47a232ffd8091ff83f8869f` / `39b70a103d99f422` | `sha256:4c3af2f6da16c64ed451ddbd5d18a96fed774e80910fa6648b34fbccd0a9ea98` | 373 | 373 | 0 | 0 | explicit-opt-in; default capture disabled |
| corrected | `d6fdf964464ecb6f4a1188c69ee4bbd2e06c3f9c` / `cb9ac22d6b26f776` | `sha256:e424cf4737884b6aeaafc8a5e9f7b606d11228eb8daeccf4336758beed1b5c48` | 2 | 1 | 1 | 1 | unrequested-unbounded-history; diagnostic-only |
| corrected | `fbf32e84662d00993c033515e113437965395494` / `21bf470589d4bcc8` | `sha256:2a9a6dc9d5f09a9785d611916f96100fe82f515f45a453bb35c83204fafb8d3e` | 120 | 120 | 0 | 120 | unrequested-unbounded-history; diagnostic-only |
| corrected | `b8c2ad4bf4fa0415fd49d57abea15729b33a4284` / `2ceb4a769c87726c` | `sha256:d7cfd5b1b29a61e724d05f2e80f368b8aa5ba08133b0c516bd5c40b6cfdd8d3b` | 5 | 4 | 1 | 5 | unrequested-unbounded-history; diagnostic-only |
| corrected | `e7840da1da81404ff228be734a91783cebb946ca` / `d156812c316b8a5e` | `sha256:53651d7989aa306d80efffb7fb98b005273b2115e7192e2e4b0c16e84602c828` | 45 | 45 | 0 | 45 | unrequested-unbounded-history; diagnostic-only |
| corrected | `3422433020a678a77f88e8a110492ca293c05e30` / `e5e43ae4a59f97bc` | `sha256:9203d33a1217f45905e74c315915be77d34d471ec3df6110a961f2d6cd4ef4c1` | 1 | 1 | 0 | 0 | unrequested-unbounded-history; diagnostic-only |
| corrected | `a40b17e05486e5b747b689e7710475d739556a69` / `a07bea45ba9806ee` | `sha256:81c5724eb5188e7e4c8cd3e92f87bb44f08caf821bff2ea4600759bbc47b7069` | 46 | 45 | 1 | 45 | unrequested-unbounded-history; diagnostic-only |
| baseline | `4c207c70f3282c316d5ab18d832504085835eda3` / `b3e378761aaad3cf` | `sha256:781f4513dcba84f51bb5b7fda4704e7e5dfe52c8aabf777b310778afba41935f` | 84 | 48 | 36 | 84 | unrequested-unbounded-history; diagnostic-only |
| corrected | `f5f8a69859bd9c0a2e7dc7780de55578fb05eec3` / `dee1c4cab283c812` | `sha256:5fe4a386bb02018caa0141621195ada5a579341011460bfd169d731f46c14c43` | 0 | 0 | 0 | 0 | explicit-opt-in; default capture disabled |
| corrected | `d1325d7f44ef205f5fa748130f3b9868973e9edc` / `2fe345784d99fb5f` | `sha256:565cf9281c49bec5a37a668dccaef448087da265f56464497f1f24dcb8c4e29e` | 5 | 4 | 1 | 4 | unrequested-unbounded-history; diagnostic-only |

## Eligible source-bound distributions

Only complete, authentic, source/input/environment-matched independently verified samples are eligible. Every source group is separate. Old unrequested SQL-history timings cannot enter these distributions. The two exact already-completed independent proofs retain their original source identity. Separately, successful clean6c samples within15seconds may retain their original timing/resource claims through the explicit product-identical, budget-only harness source bridge; this does not admit SQL-contaminated timings or claim new-observer cost equivalence. CPU/I/O use observed boundary differences; transaction and memory/spool high-water values take maxima; Store growth uses signed endpoint differences. Verification-derived sharing/storage values are labelled verified and retain their independent producing evidence.

| Arm / source group | Case | Metric | n | Median | Min | Max |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| corrected / `39b70a103d99f422` | payload-create-100m | attempted_syscall_count | 3 | 106 | 106 | 106 |
| corrected / `39b70a103d99f422` | payload-create-100m | benchmark_injection_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | benchmark_reopen_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | benchmark_verifier_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | cache_acquisition_ns | 3 | 11654958 | 11522959 | 12385375 |
| corrected / `39b70a103d99f422` | payload-create-100m | cache_build_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | cache_validation_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | candidate.admission_transactions | 3 | 42 | 42 | 42 |
| corrected / `39b70a103d99f422` | payload-create-100m | candidate.batch_inserted_bytes | 3 | 103194516 | 103194516 | 103223733 |
| corrected / `39b70a103d99f422` | payload-create-100m | candidate.batch_inserted_objects | 3 | 5334 | 5334 | 5334 |
| corrected / `39b70a103d99f422` | payload-create-100m | candidate.candidate_bytes | 3 | 105191997 | 105191997 | 105191997 |
| corrected / `39b70a103d99f422` | payload-create-100m | candidate.candidate_objects | 3 | 5452 | 5452 | 5452 |
| corrected / `39b70a103d99f422` | payload-create-100m | candidate.final_inserted_bytes | 3 | 1997258 | 1968041 | 1997258 |
| corrected / `39b70a103d99f422` | payload-create-100m | candidate.final_inserted_objects | 3 | 115 | 115 | 115 |
| corrected / `39b70a103d99f422` | payload-create-100m | candidate.inserted_bytes | 3 | 105191774 | 105191774 | 105191774 |
| corrected / `39b70a103d99f422` | payload-create-100m | candidate.inserted_objects | 3 | 5449 | 5449 | 5449 |
| corrected / `39b70a103d99f422` | payload-create-100m | candidate.max_transaction_bytes | 3 | 2567975 | 2567975 | 2568991 |
| corrected / `39b70a103d99f422` | payload-create-100m | candidate.max_transaction_objects | 3 | 127 | 127 | 127 |
| corrected / `39b70a103d99f422` | payload-create-100m | candidate.preexisting_reused_bytes | 3 | 223 | 223 | 223 |
| corrected / `39b70a103d99f422` | payload-create-100m | candidate.preexisting_reused_objects | 3 | 3 | 3 | 3 |
| corrected / `39b70a103d99f422` | payload-create-100m | candidate.reused_bytes | 3 | 223 | 223 | 223 |
| corrected / `39b70a103d99f422` | payload-create-100m | candidate.reused_objects | 3 | 3 | 3 | 3 |
| corrected / `39b70a103d99f422` | payload-create-100m | cgroup.cpu_usage_usec_delta | 3 | 99584 | 95530 | 102004 |
| corrected / `39b70a103d99f422` | payload-create-100m | cgroup.cpu_usage_usec_end | 3 | 142560 | 141671 | 145427 |
| corrected / `39b70a103d99f422` | payload-create-100m | cgroup.cpu_usage_usec_start | 3 | 43423 | 42976 | 46141 |
| corrected / `39b70a103d99f422` | payload-create-100m | cgroup.observed_max.memory.current | 3 | 9363456 | 9256960 | 10010624 |
| corrected / `39b70a103d99f422` | payload-create-100m | cgroup.observed_max.memory.events.oom | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | cgroup.observed_max.memory.events.oom_kill | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | cgroup.observed_max.memory.peak | 3 | 10436608 | 10203136 | 10575872 |
| corrected / `39b70a103d99f422` | payload-create-100m | cgroup.observed_max.memory.stat.anon | 3 | 4071424 | 4067328 | 4071424 |
| corrected / `39b70a103d99f422` | payload-create-100m | cgroup.observed_max.memory.stat.file | 3 | 4096 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | payload-create-100m | cgroup.observed_max.memory.stat.file_dirty | 3 | 4096 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | payload-create-100m | cgroup.observed_max.memory.stat.file_writeback | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | cgroup.observed_max.memory.stat.kernel | 3 | 1318912 | 1298432 | 1327104 |
| corrected / `39b70a103d99f422` | payload-create-100m | cgroup.observed_max.memory.stat.shmem | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | cgroup.observed_max.memory.stat.slab | 3 | 668600 | 668376 | 670176 |
| corrected / `39b70a103d99f422` | payload-create-100m | cgroup.observed_max.memory.swap.current | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | cgroup.observed_max.pids.current | 3 | 17 | 17 | 17 |
| corrected / `39b70a103d99f422` | payload-create-100m | cleanup_ns | 3 | 291513750 | 287446250 | 297965750 |
| corrected / `39b70a103d99f422` | payload-create-100m | clone_bytes | 3 | 917504 | 917504 | 917504 |
| corrected / `39b70a103d99f422` | payload-create-100m | clone_wall_ns | 3 | 6303208 | 6274791 | 6318083 |
| corrected / `39b70a103d99f422` | payload-create-100m | command_wall_ns | 3 | 1355504625 | 1346127167 | 1364164042 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_diagnostics.cdc_bytes_scanned | 3 | 104857600 | 104857600 | 104857600 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_diagnostics.edit_count | 3 | 100 | 100 | 100 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_diagnostics.edit_metric_nodes_scanned | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_diagnostics.edit_piece_count | 3 | 100 | 100 | 100 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_diagnostics.edit_piece_height | 3 | 16 | 16 | 16 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_diagnostics.edit_piece_logical_charge | 3 | 12800 | 12800 | 12800 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_diagnostics.edit_spool_allocated_bytes | 3 | 104857600 | 104857600 | 104857600 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_diagnostics.edit_spool_live_bytes | 3 | 104857600 | 104857600 | 104857600 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_diagnostics.edit_spool_peak_bytes | 3 | 104857600 | 104857600 | 104857600 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_diagnostics.edit_spool_superseded_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_diagnostics.edit_tree_visits | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_diagnostics.namespace_base_paths_visited | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_diagnostics.namespace_candidate_probe_nodes | 3 | 3 | 3 | 4 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_diagnostics.namespace_clean_nodes_visited | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_diagnostics.namespace_dirty_nodes_visited | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_diagnostics.namespace_final_paths_visited | 3 | 7 | 7 | 7 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_diagnostics.physical_spool_allocated_bytes | 3 | 104857600 | 104857600 | 104857600 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_diagnostics.physical_spool_observation_count | 3 | 303 | 303 | 303 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_diagnostics.physical_spool_observation_errors | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_diagnostics.physical_spool_peak_bytes | 3 | 104857600 | 104857600 | 104857600 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_ns | 3 | 275633042 | 275153791 | 283036667 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.candidate_finish_ns | 3 | 3545000 | 3128333 | 3545583 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.candidate_plan_ns | 3 | 2198291 | 1926375 | 2292708 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.capture_ns | 3 | 8792 | 8292 | 8959 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.captured_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.captured_files | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.content_ns | 3 | 174401750 | 173808417 | 180882625 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.dirty_compare_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.in_place_rebase_ns | 3 | 3598166 | 3389250 | 4272208 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.local_admission_ns | 3 | 12402458 | 12324167 | 12746625 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.max_admission_transaction_bytes | 3 | 2567975 | 2567975 | 2568991 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.max_admission_transaction_objects | 3 | 127 | 127 | 127 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.namespace_ns | 3 | 26542 | 19875 | 31417 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.object_admission_begin_ns | 3 | 186046 | 182545 | 217874 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.object_admission_commit_ns | 3 | 18858789 | 18711002 | 19339751 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.object_admission_insert_ns | 3 | 36460444 | 36122548 | 36935046 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.object_admission_ns | 3 | 75331750 | 74748041 | 76561417 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.object_admission_transactions | 3 | 42 | 42 | 42 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.pause_fence_ns | 3 | 376208 | 365750 | 420417 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.payload_bytes_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.publication_begin_ns | 3 | 3166 | 2333 | 10000 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.publication_commit_ns | 3 | 372542 | 323625 | 439834 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.publication_insert_ns | 3 | 675915 | 671534 | 701835 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.publication_metadata_ns | 3 | 143416 | 136458 | 184125 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.publication_ns | 3 | 1240917 | 1149417 | 1306792 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.publication_payload_ns | 3 | 2747 | 2419 | 2958 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.quiesce_ns | 3 | 375 | 250 | 375 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.resume_ns | 3 | 517583 | 512250 | 554417 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.snapshot_database_bytes | 3 | 1140 | 1140 | 1140 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.snapshot_database_calls | 3 | 11 | 11 | 11 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.snapshot_database_rows | 3 | 11 | 11 | 11 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.total_ns | 3 | 275620750 | 275150167 | 283026083 |
| corrected / `39b70a103d99f422` | payload-create-100m | commit_work.unattributed_ns | 3 | 2016751 | 2003458 | 2124999 |
| corrected / `39b70a103d99f422` | payload-create-100m | completed_chain_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | completed_episode_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | completed_file_write_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-100m | completed_read_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | completed_read_request_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | completed_syscall_count | 3 | 106 | 106 | 106 |
| corrected / `39b70a103d99f422` | payload-create-100m | completed_target_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | completed_write_bytes | 3 | 104857600 | 104857600 | 104857600 |
| corrected / `39b70a103d99f422` | payload-create-100m | create_ns | 3 | 8170917 | 8098000 | 10404833 |
| corrected / `39b70a103d99f422` | payload-create-100m | created_commit_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-100m | directory_entry_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | editor_save_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | end_ns | 3 | 4099750 | 3150083 | 4164042 |
| corrected / `39b70a103d99f422` | payload-create-100m | exec_ns | 3 | 186138875 | 180291584 | 200379375 |
| corrected / `39b70a103d99f422` | payload-create-100m | external_process_wall_ns | 3 | 579908000 | 574934458 | 589632291 |
| corrected / `39b70a103d99f422` | payload-create-100m | file_sync_ns | 3 | 2683667 | 1544333 | 2867292 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.callback_access | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.callback_create | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.callback_flush | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.callback_fsync | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.callback_fsyncdir | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.callback_getattr | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.callback_link | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.callback_lookup | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.callback_mkdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.callback_mknod | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.callback_open | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.callback_opendir | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.callback_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.callback_readdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.callback_readdirplus | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.callback_readlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.callback_release | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.callback_releasedir | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.callback_rename | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.callback_rmdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.callback_setattr | 3 | 4 | 4 | 4 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.callback_statfs | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.callback_symlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.callback_unlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.callback_write | 3 | 200 | 200 | 200 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.client_decode_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.client_decode_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.client_response_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.client_response_frames | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.client_socket_read_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.collection_ns | 3 | 514292 | 445625 | 516125 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.directory_entries_returned | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.directory_nonzero_offset_requests | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.host_dispatch_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.host_encode_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.host_response_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.host_response_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.host_response_frames | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.host_socket_write_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.init_capabilities | 3 | 4481057 | 4481057 | 4481057 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.kernel_read_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.kernel_read_gt_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.kernel_read_le_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.kernel_read_le_256k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.kernel_read_le_4k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.kernel_read_le_64k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.kernel_read_requests | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.local_bytes | 3 | 4807 | 4807 | 4807 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.local_calls | 3 | 52 | 52 | 52 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.local_ids | 3 | 52 | 52 | 52 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.local_read_auth_ns | 3 | 378331 | 366792 | 389671 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.local_rows | 3 | 52 | 52 | 52 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.max_payload_batch | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.max_readahead_bytes | 3 | 131072 | 131072 | 131072 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.payload_batches | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.payload_bytes_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.payload_ids | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.read_ahead_cache_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.read_ahead_fetched_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.read_ahead_fetches | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.read_ahead_hits | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.read_ahead_misses | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.read_ahead_requested_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.read_ahead_served_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.read_ahead_unused_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.read_plan_builds | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.rope_nodes_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.snapshot_cache_bytes | 3 | 3525 | 3525 | 3525 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.snapshot_cache_hits | 3 | 39 | 39 | 39 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.snapshot_cache_rows | 3 | 39 | 39 | 39 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.snapshot_database_bytes | 3 | 1282 | 1282 | 1282 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.snapshot_database_calls | 3 | 13 | 13 | 13 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.snapshot_database_rows | 3 | 13 | 13 | 13 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.workspace_output_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.workspace_read_calls | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.workspace_read_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_read.workspace_requested_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_write.client_frame_bytes | 3 | 104860100 | 104860100 | 104860100 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_write.client_request_copy_bytes | 3 | 104857600 | 104857600 | 104857600 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_write.collection_ns | 3 | 346375 | 336958 | 430292 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_write.decode_ns | 3 | 2119538 | 2063752 | 2124250 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_write.encode_ns | 3 | 4954 | 4872 | 5126 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_write.frame_payload_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_write.host_decode_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_write.host_dispatch_ns | 3 | 155738294 | 149908206 | 169767332 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_write.host_frame_bytes | 3 | 104860100 | 104860100 | 104860100 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_write.kernel_write_bytes | 3 | 104857600 | 104857600 | 104857600 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_write.kernel_write_gt_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_write.kernel_write_le_1m | 3 | 100 | 100 | 100 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_write.kernel_write_le_256k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_write.kernel_write_le_4k | 3 | 100 | 100 | 100 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_write.kernel_write_le_64k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_write.kernel_write_requests | 3 | 200 | 200 | 200 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_write.max_write_bytes | 3 | 1048576 | 1048576 | 1048576 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_write.socket_read_ns | 3 | 18770636 | 18517291 | 18839631 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_write.socket_write_ns | 3 | 67563456 | 62133846 | 84919870 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_write.spool_write_bytes | 3 | 104857600 | 104857600 | 104857600 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_write.spool_write_ns | 3 | 20234751 | 19134536 | 21135205 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_write.spool_write_open_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_write.workspace_fence_count | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-100m | fuse_write.workspace_fence_ns | 3 | 2310751 | 1236083 | 2574499 |
| corrected / `39b70a103d99f422` | payload-create-100m | git_process_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | host.disk_read_bytes.delta | 3 | 131072 | 12288 | 221184 |
| corrected / `39b70a103d99f422` | payload-create-100m | host.disk_read_bytes.end | 3 | 131072 | 12288 | 221184 |
| corrected / `39b70a103d99f422` | payload-create-100m | host.disk_read_bytes.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | host.disk_write_bytes.delta | 3 | 317571072 | 317571072 | 317571072 |
| corrected / `39b70a103d99f422` | payload-create-100m | host.disk_write_bytes.end | 3 | 317571072 | 317571072 | 317571072 |
| corrected / `39b70a103d99f422` | payload-create-100m | host.disk_write_bytes.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | host.peak_resident_bytes.max | 3 | 77463552 | 75300864 | 77496320 |
| corrected / `39b70a103d99f422` | payload-create-100m | host.physical_footprint_bytes.max | 3 | 32539128 | 30360080 | 32539176 |
| corrected / `39b70a103d99f422` | payload-create-100m | host.resident_bytes.max | 3 | 77414400 | 75251712 | 77447168 |
| corrected / `39b70a103d99f422` | payload-create-100m | host.swaps.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | host.swaps.end | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | host.swaps.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | host.system_cpu_ns.delta | 3 | 162724666 | 160655000 | 164213916 |
| corrected / `39b70a103d99f422` | payload-create-100m | host.system_cpu_ns.end | 3 | 164738916 | 162088125 | 165870791 |
| corrected / `39b70a103d99f422` | payload-create-100m | host.system_cpu_ns.start | 3 | 1656875 | 1433125 | 2014250 |
| corrected / `39b70a103d99f422` | payload-create-100m | host.user_cpu_ns.delta | 3 | 361565917 | 360143583 | 363482083 |
| corrected / `39b70a103d99f422` | payload-create-100m | host.user_cpu_ns.end | 3 | 363395125 | 361841708 | 365329291 |
| corrected / `39b70a103d99f422` | payload-create-100m | host.user_cpu_ns.start | 3 | 1829208 | 1698125 | 1847208 |
| corrected / `39b70a103d99f422` | payload-create-100m | host_orchestration_ns | 3 | 508771250 | 506046208 | 520341625 |
| corrected / `39b70a103d99f422` | payload-create-100m | host_sampler.baseline_bytes | 3 | 2588672 | 2588672 | 2605056 |
| corrected / `39b70a103d99f422` | payload-create-100m | host_sampler.final_bytes | 3 | 35635200 | 33472512 | 35651584 |
| corrected / `39b70a103d99f422` | payload-create-100m | host_sampler.maximum_gap_ns | 3 | 12530459 | 12525334 | 12533167 |
| corrected / `39b70a103d99f422` | payload-create-100m | host_sampler.sample_count | 3 | 50 | 50 | 52 |
| corrected / `39b70a103d99f422` | payload-create-100m | host_sampler.sampled_peak_bytes | 3 | 77414400 | 75251712 | 77463552 |
| corrected / `39b70a103d99f422` | payload-create-100m | inplace_edit_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | input.fixture_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | input.regular_files | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | interrupted_syscall_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | lifecycle.anchor_prefetch_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | lifecycle.cleanup_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | lifecycle.docker_calls | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | lifecycle.docker_setup_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | lifecycle.helper_copy_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | lifecycle.mount_ready_ns | 3 | 7234166 | 7079209 | 9313667 |
| corrected / `39b70a103d99f422` | payload-create-100m | lifecycle.proxy_ns | 3 | 161250 | 150042 | 176041 |
| corrected / `39b70a103d99f422` | payload-create-100m | lifecycle.small_file_prefetch_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | lifecycle.small_file_prefetch_eligible | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | lifecycle.snapshot_cache_bytes_at_create | 3 | 1548 | 1548 | 1548 |
| corrected / `39b70a103d99f422` | payload-create-100m | lifecycle.snapshot_cache_rows_at_create | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-create-100m | lifecycle.snapshot_database_bytes | 3 | 908 | 908 | 908 |
| corrected / `39b70a103d99f422` | payload-create-100m | lifecycle.snapshot_database_calls | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-create-100m | lifecycle.snapshot_database_rows | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-create-100m | lifecycle.snapshot_store_wide_scans | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | lifecycle.total_ns | 3 | 9857791 | 9091667 | 12128042 |
| corrected / `39b70a103d99f422` | payload-create-100m | lifecycle.unattributed_ns | 3 | 211873 | 196959 | 245376 |
| corrected / `39b70a103d99f422` | payload-create-100m | lifecycle.unmount_ns | 3 | 890584 | 752792 | 1219083 |
| corrected / `39b70a103d99f422` | payload-create-100m | lifecycle.wait_ns | 3 | 1173875 | 757708 | 1514875 |
| corrected / `39b70a103d99f422` | payload-create-100m | metadata_normalization_count | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-100m | metadata_normalization_ns | 3 | 701375 | 664709 | 833459 |
| corrected / `39b70a103d99f422` | payload-create-100m | orchestration_unattributed_ns | 3 | 30354499 | 29741458 | 36030334 |
| corrected / `39b70a103d99f422` | payload-create-100m | preparation_ns | 3 | 473397917 | 467357542 | 500837333 |
| corrected / `39b70a103d99f422` | payload-create-100m | pure_call_sum_ns | 3 | 475691709 | 472740916 | 490600167 |
| corrected / `39b70a103d99f422` | payload-create-100m | root_sync_ns | 3 | 397125 | 374084 | 547292 |
| corrected / `39b70a103d99f422` | payload-create-100m | runtime_preparation_ns | 3 | 371474708 | 370245167 | 403614292 |
| corrected / `39b70a103d99f422` | payload-create-100m | spool_boundary.max_allocated_bytes | 3 | 104861696 | 104861696 | 104861696 |
| corrected / `39b70a103d99f422` | payload-create-100m | spool_boundary.max_file_count | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-100m | spool_boundary.max_logical_bytes | 3 | 104857747 | 104857747 | 104857747 |
| corrected / `39b70a103d99f422` | payload-create-100m | store.allocated_bytes.delta | 3 | 133365760 | 133365760 | 133365760 |
| corrected / `39b70a103d99f422` | payload-create-100m | store.allocated_bytes.end | 3 | 134283264 | 134283264 | 134283264 |
| corrected / `39b70a103d99f422` | payload-create-100m | store.allocated_bytes.max | 3 | 134283264 | 134283264 | 134283264 |
| corrected / `39b70a103d99f422` | payload-create-100m | store.allocated_bytes.start | 3 | 917504 | 917504 | 917504 |
| corrected / `39b70a103d99f422` | payload-create-100m | store.file_bytes.delta | 3 | 124911616 | 124911616 | 124911616 |
| corrected / `39b70a103d99f422` | payload-create-100m | store.file_bytes.end | 3 | 125829120 | 125829120 | 125829120 |
| corrected / `39b70a103d99f422` | payload-create-100m | store.file_bytes.max | 3 | 125829120 | 125829120 | 125829120 |
| corrected / `39b70a103d99f422` | payload-create-100m | store.file_bytes.start | 3 | 917504 | 917504 | 917504 |
| corrected / `39b70a103d99f422` | payload-create-100m | store.freelist_page_count.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | store.freelist_page_count.end | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | store.freelist_page_count.max | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | store.freelist_page_count.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | store.live_page_bytes.delta | 3 | 124911616 | 124911616 | 124911616 |
| corrected / `39b70a103d99f422` | payload-create-100m | store.live_page_bytes.end | 3 | 125829120 | 125829120 | 125829120 |
| corrected / `39b70a103d99f422` | payload-create-100m | store.live_page_bytes.max | 3 | 125829120 | 125829120 | 125829120 |
| corrected / `39b70a103d99f422` | payload-create-100m | store.live_page_bytes.start | 3 | 917504 | 917504 | 917504 |
| corrected / `39b70a103d99f422` | payload-create-100m | store.page_count.delta | 3 | 1906 | 1906 | 1906 |
| corrected / `39b70a103d99f422` | payload-create-100m | store.page_count.end | 3 | 1920 | 1920 | 1920 |
| corrected / `39b70a103d99f422` | payload-create-100m | store.page_count.max | 3 | 1920 | 1920 | 1920 |
| corrected / `39b70a103d99f422` | payload-create-100m | store.page_count.start | 3 | 14 | 14 | 14 |
| corrected / `39b70a103d99f422` | payload-create-100m | verified.canonical-verification.canonical_Chunk_bytes | 3 | 104970957 | 104970957 | 104970957 |
| corrected / `39b70a103d99f422` | payload-create-100m | verified.canonical-verification.canonical_Chunk_objects | 3 | 5397 | 5397 | 5397 |
| corrected / `39b70a103d99f422` | payload-create-100m | verified.canonical-verification.canonical_DirectoryNode_bytes | 3 | 89 | 89 | 89 |
| corrected / `39b70a103d99f422` | payload-create-100m | verified.canonical-verification.canonical_DirectoryNode_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-100m | verified.canonical-verification.canonical_DirectoryState_bytes | 3 | 98 | 98 | 98 |
| corrected / `39b70a103d99f422` | payload-create-100m | verified.canonical-verification.canonical_DirectoryState_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-100m | verified.canonical-verification.canonical_FileNode_bytes | 3 | 220012 | 220012 | 220012 |
| corrected / `39b70a103d99f422` | payload-create-100m | verified.canonical-verification.canonical_FileNode_objects | 3 | 47 | 47 | 47 |
| corrected / `39b70a103d99f422` | payload-create-100m | verified.canonical-verification.canonical_FileState_bytes | 3 | 424 | 424 | 424 |
| corrected / `39b70a103d99f422` | payload-create-100m | verified.canonical-verification.canonical_FileState_objects | 3 | 4 | 4 | 4 |
| corrected / `39b70a103d99f422` | payload-create-100m | verified.canonical-verification.canonical_InodeRecord_bytes | 3 | 196 | 196 | 196 |
| corrected / `39b70a103d99f422` | payload-create-100m | verified.canonical-verification.canonical_InodeRecord_objects | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-100m | verified.canonical-verification.canonical_InodeTable_bytes | 3 | 172 | 172 | 172 |
| corrected / `39b70a103d99f422` | payload-create-100m | verified.canonical-verification.canonical_InodeTable_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-100m | verified.canonical-verification.canonical_Metadata_bytes | 3 | 286 | 286 | 286 |
| corrected / `39b70a103d99f422` | payload-create-100m | verified.canonical-verification.canonical_Metadata_objects | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-100m | verified.canonical-verification.canonical_Namespace_bytes | 3 | 121 | 121 | 121 |
| corrected / `39b70a103d99f422` | payload-create-100m | verified.canonical-verification.canonical_Namespace_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-100m | verified.canonical-verification.canonical_unique_bytes | 3 | 105192355 | 105192355 | 105192355 |
| corrected / `39b70a103d99f422` | payload-create-100m | verified.canonical-verification.canonical_unique_objects | 3 | 5456 | 5456 | 5456 |
| corrected / `39b70a103d99f422` | payload-create-100m | verified.canonical-verification.independent_content_paths | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-100m | verified.canonical-verification.logical_bytes | 3 | 104857600 | 104857600 | 104857600 |
| corrected / `39b70a103d99f422` | payload-create-100m | verified.canonical-verification.persistence_custody_paths | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | verified.canonical-verification.verified_paths | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-100m | verified.canonical-verification.verified_regular_paths | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-100m | visibility_ns | 3 | 101416 | 83167 | 127250 |
| corrected / `39b70a103d99f422` | payload-create-100m | visited_file_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | visited_path_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | workload_chmod_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | workload_close_call_count | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-100m | workload_closedir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | workload_fsync_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-100m | workload_fsyncdir_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-100m | workload_ftruncate_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | workload_lstat_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | workload_mkdir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | workload_ns | 3 | 182039542 | 176709042 | 196112666 |
| corrected / `39b70a103d99f422` | payload-create-100m | workload_open_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-100m | workload_open_directory_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-100m | workload_opendir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | workload_plan_ns | 3 | 250 | 83 | 250 |
| corrected / `39b70a103d99f422` | payload-create-100m | workload_pread_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | workload_pwrite_call_count | 3 | 100 | 100 | 100 |
| corrected / `39b70a103d99f422` | payload-create-100m | workload_rename_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | workload_rmdir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | workload_symlink_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-100m | workload_unlink_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | attempted_syscall_count | 3 | 16 | 16 | 16 |
| corrected / `39b70a103d99f422` | payload-create-10m | benchmark_injection_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | benchmark_reopen_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | benchmark_verifier_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | cache_acquisition_ns | 3 | 12323583 | 12318458 | 16313667 |
| corrected / `39b70a103d99f422` | payload-create-10m | cache_build_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | cache_validation_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | candidate.admission_transactions | 3 | 4 | 4 | 4 |
| corrected / `39b70a103d99f422` | payload-create-10m | candidate.batch_inserted_bytes | 3 | 9738885 | 9738885 | 9777498 |
| corrected / `39b70a103d99f422` | payload-create-10m | candidate.batch_inserted_objects | 3 | 508 | 508 | 508 |
| corrected / `39b70a103d99f422` | payload-create-10m | candidate.candidate_bytes | 3 | 10520811 | 10520811 | 10520811 |
| corrected / `39b70a103d99f422` | payload-create-10m | candidate.candidate_objects | 3 | 564 | 564 | 564 |
| corrected / `39b70a103d99f422` | payload-create-10m | candidate.final_inserted_bytes | 3 | 781703 | 743090 | 781703 |
| corrected / `39b70a103d99f422` | payload-create-10m | candidate.final_inserted_objects | 3 | 53 | 53 | 53 |
| corrected / `39b70a103d99f422` | payload-create-10m | candidate.inserted_bytes | 3 | 10520588 | 10520588 | 10520588 |
| corrected / `39b70a103d99f422` | payload-create-10m | candidate.inserted_objects | 3 | 561 | 561 | 561 |
| corrected / `39b70a103d99f422` | payload-create-10m | candidate.max_transaction_bytes | 3 | 2489975 | 2489975 | 2491534 |
| corrected / `39b70a103d99f422` | payload-create-10m | candidate.max_transaction_objects | 3 | 127 | 127 | 127 |
| corrected / `39b70a103d99f422` | payload-create-10m | candidate.preexisting_reused_bytes | 3 | 223 | 223 | 223 |
| corrected / `39b70a103d99f422` | payload-create-10m | candidate.preexisting_reused_objects | 3 | 3 | 3 | 3 |
| corrected / `39b70a103d99f422` | payload-create-10m | candidate.reused_bytes | 3 | 223 | 223 | 223 |
| corrected / `39b70a103d99f422` | payload-create-10m | candidate.reused_objects | 3 | 3 | 3 | 3 |
| corrected / `39b70a103d99f422` | payload-create-10m | cgroup.cpu_usage_usec_delta | 3 | 18962 | 18883 | 19415 |
| corrected / `39b70a103d99f422` | payload-create-10m | cgroup.cpu_usage_usec_end | 3 | 62294 | 62132 | 62419 |
| corrected / `39b70a103d99f422` | payload-create-10m | cgroup.cpu_usage_usec_start | 3 | 43249 | 43004 | 43332 |
| corrected / `39b70a103d99f422` | payload-create-10m | cgroup.observed_max.memory.current | 3 | 5820416 | 5738496 | 6012928 |
| corrected / `39b70a103d99f422` | payload-create-10m | cgroup.observed_max.memory.events.oom | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | cgroup.observed_max.memory.events.oom_kill | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | cgroup.observed_max.memory.peak | 3 | 6934528 | 6819840 | 6950912 |
| corrected / `39b70a103d99f422` | payload-create-10m | cgroup.observed_max.memory.stat.anon | 3 | 4071424 | 4067328 | 4075520 |
| corrected / `39b70a103d99f422` | payload-create-10m | cgroup.observed_max.memory.stat.file | 3 | 4096 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | payload-create-10m | cgroup.observed_max.memory.stat.file_dirty | 3 | 4096 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | payload-create-10m | cgroup.observed_max.memory.stat.file_writeback | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | cgroup.observed_max.memory.stat.kernel | 3 | 1306624 | 1306624 | 1310720 |
| corrected / `39b70a103d99f422` | payload-create-10m | cgroup.observed_max.memory.stat.shmem | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | cgroup.observed_max.memory.stat.slab | 3 | 670544 | 669208 | 671288 |
| corrected / `39b70a103d99f422` | payload-create-10m | cgroup.observed_max.memory.swap.current | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | cgroup.observed_max.pids.current | 3 | 17 | 17 | 17 |
| corrected / `39b70a103d99f422` | payload-create-10m | cleanup_ns | 3 | 302206542 | 292333083 | 312294917 |
| corrected / `39b70a103d99f422` | payload-create-10m | clone_bytes | 3 | 917504 | 917504 | 917504 |
| corrected / `39b70a103d99f422` | payload-create-10m | clone_wall_ns | 3 | 6029709 | 6021125 | 6262833 |
| corrected / `39b70a103d99f422` | payload-create-10m | command_wall_ns | 3 | 935464875 | 935292375 | 986397417 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_diagnostics.cdc_bytes_scanned | 3 | 10485760 | 10485760 | 10485760 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_diagnostics.edit_count | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_diagnostics.edit_metric_nodes_scanned | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_diagnostics.edit_piece_count | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_diagnostics.edit_piece_height | 3 | 4 | 4 | 4 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_diagnostics.edit_piece_logical_charge | 3 | 1280 | 1280 | 1280 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_diagnostics.edit_spool_allocated_bytes | 3 | 10485760 | 10485760 | 10485760 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_diagnostics.edit_spool_live_bytes | 3 | 10485760 | 10485760 | 10485760 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_diagnostics.edit_spool_peak_bytes | 3 | 10485760 | 10485760 | 10485760 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_diagnostics.edit_spool_superseded_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_diagnostics.edit_tree_visits | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_diagnostics.namespace_base_paths_visited | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_diagnostics.namespace_candidate_probe_nodes | 3 | 3 | 3 | 4 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_diagnostics.namespace_clean_nodes_visited | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_diagnostics.namespace_dirty_nodes_visited | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_diagnostics.namespace_final_paths_visited | 3 | 7 | 7 | 7 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_diagnostics.physical_spool_allocated_bytes | 3 | 10485760 | 10485760 | 10485760 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_diagnostics.physical_spool_observation_count | 3 | 33 | 33 | 33 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_diagnostics.physical_spool_observation_errors | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_diagnostics.physical_spool_peak_bytes | 3 | 10485760 | 10485760 | 10485760 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_ns | 3 | 39640167 | 39086292 | 39889333 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.candidate_finish_ns | 3 | 5237875 | 4941500 | 5251708 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.candidate_plan_ns | 3 | 542250 | 446750 | 1119917 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.capture_ns | 3 | 10250 | 7708 | 12708 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.captured_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.captured_files | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.content_ns | 3 | 18347833 | 18041834 | 18865250 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.dirty_compare_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.in_place_rebase_ns | 3 | 2024209 | 1083500 | 2155708 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.local_admission_ns | 3 | 1435750 | 1405250 | 1440208 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.max_admission_transaction_bytes | 3 | 2489975 | 2489975 | 2491534 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.max_admission_transaction_objects | 3 | 127 | 127 | 127 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.namespace_ns | 3 | 23792 | 18250 | 27667 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.object_admission_begin_ns | 3 | 30499 | 28083 | 63915 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.object_admission_commit_ns | 3 | 2777000 | 2740333 | 2853500 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.object_admission_insert_ns | 3 | 4085697 | 4066584 | 4182530 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.object_admission_ns | 3 | 9246125 | 9153250 | 9799167 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.object_admission_transactions | 3 | 4 | 4 | 4 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.pause_fence_ns | 3 | 348000 | 333917 | 362625 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.payload_bytes_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.publication_begin_ns | 3 | 5584 | 5333 | 9291 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.publication_commit_ns | 3 | 188708 | 177584 | 205625 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.publication_insert_ns | 3 | 359878 | 347832 | 365669 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.publication_metadata_ns | 3 | 107791 | 97792 | 113417 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.publication_ns | 3 | 660541 | 651917 | 688291 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.publication_payload_ns | 3 | 1286 | 1208 | 1461 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.quiesce_ns | 3 | 542 | 375 | 708 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.resume_ns | 3 | 538209 | 461750 | 1427833 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.snapshot_database_bytes | 3 | 1140 | 1140 | 1140 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.snapshot_database_calls | 3 | 11 | 11 | 11 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.snapshot_database_rows | 3 | 11 | 11 | 11 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.total_ns | 3 | 39635875 | 39077542 | 39885250 |
| corrected / `39b70a103d99f422` | payload-create-10m | commit_work.unattributed_ns | 3 | 762751 | 751873 | 970876 |
| corrected / `39b70a103d99f422` | payload-create-10m | completed_chain_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | completed_episode_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | completed_file_write_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-10m | completed_read_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | completed_read_request_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | completed_syscall_count | 3 | 16 | 16 | 16 |
| corrected / `39b70a103d99f422` | payload-create-10m | completed_target_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | completed_write_bytes | 3 | 10485760 | 10485760 | 10485760 |
| corrected / `39b70a103d99f422` | payload-create-10m | create_ns | 3 | 9904000 | 9265209 | 10699916 |
| corrected / `39b70a103d99f422` | payload-create-10m | created_commit_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-10m | directory_entry_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | editor_save_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | end_ns | 3 | 3532167 | 3270792 | 4306167 |
| corrected / `39b70a103d99f422` | payload-create-10m | exec_ns | 3 | 30573750 | 30192209 | 33003500 |
| corrected / `39b70a103d99f422` | payload-create-10m | external_process_wall_ns | 3 | 159092459 | 158097625 | 160211208 |
| corrected / `39b70a103d99f422` | payload-create-10m | file_sync_ns | 3 | 3651333 | 3572667 | 3660667 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.callback_access | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.callback_create | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.callback_flush | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.callback_fsync | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.callback_fsyncdir | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.callback_getattr | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.callback_link | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.callback_lookup | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.callback_mkdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.callback_mknod | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.callback_open | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.callback_opendir | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.callback_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.callback_readdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.callback_readdirplus | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.callback_readlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.callback_release | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.callback_releasedir | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.callback_rename | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.callback_rmdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.callback_setattr | 3 | 4 | 4 | 4 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.callback_statfs | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.callback_symlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.callback_unlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.callback_write | 3 | 20 | 20 | 20 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.client_decode_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.client_decode_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.client_response_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.client_response_frames | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.client_socket_read_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.collection_ns | 3 | 537291 | 530000 | 681708 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.directory_entries_returned | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.directory_nonzero_offset_requests | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.host_dispatch_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.host_encode_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.host_response_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.host_response_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.host_response_frames | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.host_socket_write_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.init_capabilities | 3 | 4481057 | 4481057 | 4481057 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.kernel_read_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.kernel_read_gt_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.kernel_read_le_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.kernel_read_le_256k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.kernel_read_le_4k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.kernel_read_le_64k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.kernel_read_requests | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.local_bytes | 3 | 4807 | 4807 | 4807 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.local_calls | 3 | 52 | 52 | 52 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.local_ids | 3 | 52 | 52 | 52 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.local_read_auth_ns | 3 | 399832 | 371915 | 624999 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.local_rows | 3 | 52 | 52 | 52 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.max_payload_batch | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.max_readahead_bytes | 3 | 131072 | 131072 | 131072 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.payload_batches | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.payload_bytes_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.payload_ids | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.read_ahead_cache_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.read_ahead_fetched_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.read_ahead_fetches | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.read_ahead_hits | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.read_ahead_misses | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.read_ahead_requested_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.read_ahead_served_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.read_ahead_unused_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.read_plan_builds | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.rope_nodes_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.snapshot_cache_bytes | 3 | 3525 | 3525 | 3525 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.snapshot_cache_hits | 3 | 39 | 39 | 39 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.snapshot_cache_rows | 3 | 39 | 39 | 39 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.snapshot_database_bytes | 3 | 1282 | 1282 | 1282 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.snapshot_database_calls | 3 | 13 | 13 | 13 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.snapshot_database_rows | 3 | 13 | 13 | 13 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.workspace_output_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.workspace_read_calls | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.workspace_read_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_read.workspace_requested_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_write.client_frame_bytes | 3 | 10486010 | 10486010 | 10486010 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_write.client_request_copy_bytes | 3 | 10485760 | 10485760 | 10485760 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_write.collection_ns | 3 | 390125 | 369583 | 543750 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_write.decode_ns | 3 | 200291 | 125125 | 203584 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_write.encode_ns | 3 | 624 | 584 | 627 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_write.frame_payload_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_write.host_decode_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_write.host_dispatch_ns | 3 | 12315666 | 9808252 | 15580000 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_write.host_frame_bytes | 3 | 10486010 | 10486010 | 10486010 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_write.kernel_write_bytes | 3 | 10485760 | 10485760 | 10485760 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_write.kernel_write_gt_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_write.kernel_write_le_1m | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_write.kernel_write_le_256k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_write.kernel_write_le_4k | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_write.kernel_write_le_64k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_write.kernel_write_requests | 3 | 20 | 20 | 20 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_write.max_write_bytes | 3 | 1048576 | 1048576 | 1048576 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_write.socket_read_ns | 3 | 8301164 | 5463125 | 12100501 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_write.socket_write_ns | 3 | 2101418 | 1858456 | 2523916 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_write.spool_write_bytes | 3 | 10485760 | 10485760 | 10485760 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_write.spool_write_ns | 3 | 2570957 | 2402415 | 2595666 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_write.spool_write_open_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_write.workspace_fence_count | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-10m | fuse_write.workspace_fence_ns | 3 | 3196625 | 2992042 | 3384417 |
| corrected / `39b70a103d99f422` | payload-create-10m | git_process_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | host.disk_read_bytes.delta | 3 | 81920 | 81920 | 90112 |
| corrected / `39b70a103d99f422` | payload-create-10m | host.disk_read_bytes.end | 3 | 81920 | 81920 | 90112 |
| corrected / `39b70a103d99f422` | payload-create-10m | host.disk_read_bytes.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | host.disk_write_bytes.delta | 3 | 33865728 | 33865728 | 33869824 |
| corrected / `39b70a103d99f422` | payload-create-10m | host.disk_write_bytes.end | 3 | 33865728 | 33865728 | 33869824 |
| corrected / `39b70a103d99f422` | payload-create-10m | host.disk_write_bytes.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | host.peak_resident_bytes.max | 3 | 45809664 | 45514752 | 46792704 |
| corrected / `39b70a103d99f422` | payload-create-10m | host.physical_footprint_bytes.max | 3 | 26247648 | 25952760 | 27247120 |
| corrected / `39b70a103d99f422` | payload-create-10m | host.resident_bytes.max | 3 | 45776896 | 45465600 | 46743552 |
| corrected / `39b70a103d99f422` | payload-create-10m | host.swaps.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | host.swaps.end | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | host.swaps.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | host.system_cpu_ns.delta | 3 | 47489541 | 46899875 | 48724041 |
| corrected / `39b70a103d99f422` | payload-create-10m | host.system_cpu_ns.end | 3 | 49248916 | 48873666 | 50358541 |
| corrected / `39b70a103d99f422` | payload-create-10m | host.system_cpu_ns.start | 3 | 1759375 | 1634500 | 1973791 |
| corrected / `39b70a103d99f422` | payload-create-10m | host.user_cpu_ns.delta | 3 | 47117959 | 46835417 | 47438541 |
| corrected / `39b70a103d99f422` | payload-create-10m | host.user_cpu_ns.end | 3 | 49056750 | 48656833 | 49259541 |
| corrected / `39b70a103d99f422` | payload-create-10m | host.user_cpu_ns.start | 3 | 1821416 | 1821000 | 1938791 |
| corrected / `39b70a103d99f422` | payload-create-10m | host_orchestration_ns | 3 | 113107625 | 112870000 | 117262125 |
| corrected / `39b70a103d99f422` | payload-create-10m | host_sampler.baseline_bytes | 3 | 2588672 | 2588672 | 2605056 |
| corrected / `39b70a103d99f422` | payload-create-10m | host_sampler.final_bytes | 3 | 29376512 | 29081600 | 30375936 |
| corrected / `39b70a103d99f422` | payload-create-10m | host_sampler.maximum_gap_ns | 3 | 12528209 | 12138666 | 12540167 |
| corrected / `39b70a103d99f422` | payload-create-10m | host_sampler.sample_count | 3 | 14 | 14 | 14 |
| corrected / `39b70a103d99f422` | payload-create-10m | host_sampler.sampled_peak_bytes | 3 | 45776896 | 45465600 | 46776320 |
| corrected / `39b70a103d99f422` | payload-create-10m | inplace_edit_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | input.fixture_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | input.regular_files | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | interrupted_syscall_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | lifecycle.anchor_prefetch_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | lifecycle.cleanup_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | lifecycle.docker_calls | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | lifecycle.docker_setup_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | lifecycle.helper_copy_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | lifecycle.mount_ready_ns | 3 | 8185375 | 8167750 | 9603833 |
| corrected / `39b70a103d99f422` | payload-create-10m | lifecycle.proxy_ns | 3 | 175459 | 169625 | 182042 |
| corrected / `39b70a103d99f422` | payload-create-10m | lifecycle.small_file_prefetch_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | lifecycle.small_file_prefetch_eligible | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | lifecycle.snapshot_cache_bytes_at_create | 3 | 1548 | 1548 | 1548 |
| corrected / `39b70a103d99f422` | payload-create-10m | lifecycle.snapshot_cache_rows_at_create | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-create-10m | lifecycle.snapshot_database_bytes | 3 | 908 | 908 | 908 |
| corrected / `39b70a103d99f422` | payload-create-10m | lifecycle.snapshot_database_calls | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-create-10m | lifecycle.snapshot_database_rows | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-create-10m | lifecycle.snapshot_store_wide_scans | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | lifecycle.total_ns | 3 | 10522750 | 10174292 | 12382458 |
| corrected / `39b70a103d99f422` | payload-create-10m | lifecycle.unattributed_ns | 3 | 237083 | 223751 | 271957 |
| corrected / `39b70a103d99f422` | payload-create-10m | lifecycle.unmount_ns | 3 | 761416 | 730083 | 833334 |
| corrected / `39b70a103d99f422` | payload-create-10m | lifecycle.wait_ns | 3 | 1067667 | 834125 | 1636000 |
| corrected / `39b70a103d99f422` | payload-create-10m | metadata_normalization_count | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-10m | metadata_normalization_ns | 3 | 652500 | 625458 | 669708 |
| corrected / `39b70a103d99f422` | payload-create-10m | orchestration_unattributed_ns | 3 | 30075333 | 29384250 | 30120748 |
| corrected / `39b70a103d99f422` | payload-create-10m | preparation_ns | 3 | 482839916 | 471874250 | 515151042 |
| corrected / `39b70a103d99f422` | payload-create-10m | pure_call_sum_ns | 3 | 83485750 | 82986877 | 87186792 |
| corrected / `39b70a103d99f422` | payload-create-10m | root_sync_ns | 3 | 395583 | 298041 | 419750 |
| corrected / `39b70a103d99f422` | payload-create-10m | runtime_preparation_ns | 3 | 381083000 | 376381500 | 402821917 |
| corrected / `39b70a103d99f422` | payload-create-10m | spool_boundary.max_allocated_bytes | 3 | 10489856 | 10489856 | 10489856 |
| corrected / `39b70a103d99f422` | payload-create-10m | spool_boundary.max_file_count | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-10m | spool_boundary.max_logical_bytes | 3 | 10485906 | 10485906 | 10485906 |
| corrected / `39b70a103d99f422` | payload-create-10m | store.allocated_bytes.delta | 3 | 12779520 | 12779520 | 12779520 |
| corrected / `39b70a103d99f422` | payload-create-10m | store.allocated_bytes.end | 3 | 13697024 | 13697024 | 13697024 |
| corrected / `39b70a103d99f422` | payload-create-10m | store.allocated_bytes.max | 3 | 13697024 | 13697024 | 13697024 |
| corrected / `39b70a103d99f422` | payload-create-10m | store.allocated_bytes.start | 3 | 917504 | 917504 | 917504 |
| corrected / `39b70a103d99f422` | payload-create-10m | store.file_bytes.delta | 3 | 12582912 | 12582912 | 12582912 |
| corrected / `39b70a103d99f422` | payload-create-10m | store.file_bytes.end | 3 | 13500416 | 13500416 | 13500416 |
| corrected / `39b70a103d99f422` | payload-create-10m | store.file_bytes.max | 3 | 13500416 | 13500416 | 13500416 |
| corrected / `39b70a103d99f422` | payload-create-10m | store.file_bytes.start | 3 | 917504 | 917504 | 917504 |
| corrected / `39b70a103d99f422` | payload-create-10m | store.freelist_page_count.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | store.freelist_page_count.end | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | store.freelist_page_count.max | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | store.freelist_page_count.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | store.live_page_bytes.delta | 3 | 12582912 | 12582912 | 12582912 |
| corrected / `39b70a103d99f422` | payload-create-10m | store.live_page_bytes.end | 3 | 13500416 | 13500416 | 13500416 |
| corrected / `39b70a103d99f422` | payload-create-10m | store.live_page_bytes.max | 3 | 13500416 | 13500416 | 13500416 |
| corrected / `39b70a103d99f422` | payload-create-10m | store.live_page_bytes.start | 3 | 917504 | 917504 | 917504 |
| corrected / `39b70a103d99f422` | payload-create-10m | store.page_count.delta | 3 | 192 | 192 | 192 |
| corrected / `39b70a103d99f422` | payload-create-10m | store.page_count.end | 3 | 206 | 206 | 206 |
| corrected / `39b70a103d99f422` | payload-create-10m | store.page_count.max | 3 | 206 | 206 | 206 |
| corrected / `39b70a103d99f422` | payload-create-10m | store.page_count.start | 3 | 14 | 14 | 14 |
| corrected / `39b70a103d99f422` | payload-create-10m | verified.canonical-verification.canonical_Chunk_bytes | 3 | 10497267 | 10497267 | 10497267 |
| corrected / `39b70a103d99f422` | payload-create-10m | verified.canonical-verification.canonical_Chunk_objects | 3 | 547 | 547 | 547 |
| corrected / `39b70a103d99f422` | payload-create-10m | verified.canonical-verification.canonical_DirectoryNode_bytes | 3 | 89 | 89 | 89 |
| corrected / `39b70a103d99f422` | payload-create-10m | verified.canonical-verification.canonical_DirectoryNode_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-10m | verified.canonical-verification.canonical_DirectoryState_bytes | 3 | 98 | 98 | 98 |
| corrected / `39b70a103d99f422` | payload-create-10m | verified.canonical-verification.canonical_DirectoryState_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-10m | verified.canonical-verification.canonical_FileNode_bytes | 3 | 22516 | 22516 | 22516 |
| corrected / `39b70a103d99f422` | payload-create-10m | verified.canonical-verification.canonical_FileNode_objects | 3 | 9 | 9 | 9 |
| corrected / `39b70a103d99f422` | payload-create-10m | verified.canonical-verification.canonical_FileState_bytes | 3 | 424 | 424 | 424 |
| corrected / `39b70a103d99f422` | payload-create-10m | verified.canonical-verification.canonical_FileState_objects | 3 | 4 | 4 | 4 |
| corrected / `39b70a103d99f422` | payload-create-10m | verified.canonical-verification.canonical_InodeRecord_bytes | 3 | 196 | 196 | 196 |
| corrected / `39b70a103d99f422` | payload-create-10m | verified.canonical-verification.canonical_InodeRecord_objects | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-10m | verified.canonical-verification.canonical_InodeTable_bytes | 3 | 172 | 172 | 172 |
| corrected / `39b70a103d99f422` | payload-create-10m | verified.canonical-verification.canonical_InodeTable_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-10m | verified.canonical-verification.canonical_Metadata_bytes | 3 | 286 | 286 | 286 |
| corrected / `39b70a103d99f422` | payload-create-10m | verified.canonical-verification.canonical_Metadata_objects | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-10m | verified.canonical-verification.canonical_Namespace_bytes | 3 | 121 | 121 | 121 |
| corrected / `39b70a103d99f422` | payload-create-10m | verified.canonical-verification.canonical_Namespace_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-10m | verified.canonical-verification.canonical_unique_bytes | 3 | 10521169 | 10521169 | 10521169 |
| corrected / `39b70a103d99f422` | payload-create-10m | verified.canonical-verification.canonical_unique_objects | 3 | 568 | 568 | 568 |
| corrected / `39b70a103d99f422` | payload-create-10m | verified.canonical-verification.independent_content_paths | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-10m | verified.canonical-verification.logical_bytes | 3 | 10485760 | 10485760 | 10485760 |
| corrected / `39b70a103d99f422` | payload-create-10m | verified.canonical-verification.persistence_custody_paths | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | verified.canonical-verification.verified_paths | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-10m | verified.canonical-verification.verified_regular_paths | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-10m | visibility_ns | 3 | 97041 | 90917 | 107959 |
| corrected / `39b70a103d99f422` | payload-create-10m | visited_file_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | visited_path_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | workload_chmod_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | workload_close_call_count | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-10m | workload_closedir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | workload_fsync_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-10m | workload_fsyncdir_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-10m | workload_ftruncate_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | workload_lstat_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | workload_mkdir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | workload_ns | 3 | 27252083 | 26887209 | 29120834 |
| corrected / `39b70a103d99f422` | payload-create-10m | workload_open_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-10m | workload_open_directory_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-10m | workload_opendir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | workload_plan_ns | 3 | 250 | 209 | 292 |
| corrected / `39b70a103d99f422` | payload-create-10m | workload_pread_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | workload_pwrite_call_count | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-create-10m | workload_rename_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | workload_rmdir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | workload_symlink_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-10m | workload_unlink_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | attempted_syscall_count | 2 | 7.0 | 7 | 7 |
| corrected / `39b70a103d99f422` | payload-create-1m | benchmark_injection_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | benchmark_reopen_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | benchmark_verifier_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | cache_acquisition_ns | 2 | 12358458.0 | 11837125 | 12879791 |
| corrected / `39b70a103d99f422` | payload-create-1m | cache_build_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | cache_validation_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | candidate.admission_transactions | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | candidate.batch_inserted_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | candidate.batch_inserted_objects | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | candidate.candidate_bytes | 2 | 1053277.0 | 1053277 | 1053277 |
| corrected / `39b70a103d99f422` | payload-create-1m | candidate.candidate_objects | 2 | 69.0 | 69 | 69 |
| corrected / `39b70a103d99f422` | payload-create-1m | candidate.final_inserted_bytes | 2 | 1053054.0 | 1053054 | 1053054 |
| corrected / `39b70a103d99f422` | payload-create-1m | candidate.final_inserted_objects | 2 | 66.0 | 66 | 66 |
| corrected / `39b70a103d99f422` | payload-create-1m | candidate.inserted_bytes | 2 | 1053054.0 | 1053054 | 1053054 |
| corrected / `39b70a103d99f422` | payload-create-1m | candidate.inserted_objects | 2 | 66.0 | 66 | 66 |
| corrected / `39b70a103d99f422` | payload-create-1m | candidate.max_transaction_bytes | 2 | 1053054.0 | 1053054 | 1053054 |
| corrected / `39b70a103d99f422` | payload-create-1m | candidate.max_transaction_objects | 2 | 66.0 | 66 | 66 |
| corrected / `39b70a103d99f422` | payload-create-1m | candidate.preexisting_reused_bytes | 2 | 223.0 | 223 | 223 |
| corrected / `39b70a103d99f422` | payload-create-1m | candidate.preexisting_reused_objects | 2 | 3.0 | 3 | 3 |
| corrected / `39b70a103d99f422` | payload-create-1m | candidate.reused_bytes | 2 | 223.0 | 223 | 223 |
| corrected / `39b70a103d99f422` | payload-create-1m | candidate.reused_objects | 2 | 3.0 | 3 | 3 |
| corrected / `39b70a103d99f422` | payload-create-1m | cgroup.cpu_usage_usec_delta | 2 | 9552.5 | 9476 | 9629 |
| corrected / `39b70a103d99f422` | payload-create-1m | cgroup.cpu_usage_usec_end | 2 | 53054.5 | 52907 | 53202 |
| corrected / `39b70a103d99f422` | payload-create-1m | cgroup.cpu_usage_usec_start | 2 | 43502.0 | 43431 | 43573 |
| corrected / `39b70a103d99f422` | payload-create-1m | cgroup.observed_max.memory.current | 2 | 4233216.0 | 4218880 | 4247552 |
| corrected / `39b70a103d99f422` | payload-create-1m | cgroup.observed_max.memory.events.oom | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | cgroup.observed_max.memory.events.oom_kill | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | cgroup.observed_max.memory.peak | 2 | 6924288.0 | 6582272 | 7266304 |
| corrected / `39b70a103d99f422` | payload-create-1m | cgroup.observed_max.memory.stat.anon | 2 | 2824192.0 | 2822144 | 2826240 |
| corrected / `39b70a103d99f422` | payload-create-1m | cgroup.observed_max.memory.stat.file | 2 | 4096.0 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | payload-create-1m | cgroup.observed_max.memory.stat.file_dirty | 2 | 4096.0 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | payload-create-1m | cgroup.observed_max.memory.stat.file_writeback | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | cgroup.observed_max.memory.stat.kernel | 2 | 1292288.0 | 1290240 | 1294336 |
| corrected / `39b70a103d99f422` | payload-create-1m | cgroup.observed_max.memory.stat.shmem | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | cgroup.observed_max.memory.stat.slab | 2 | 651300.0 | 650464 | 652136 |
| corrected / `39b70a103d99f422` | payload-create-1m | cgroup.observed_max.memory.swap.current | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | cgroup.observed_max.pids.current | 2 | 15.0 | 13 | 17 |
| corrected / `39b70a103d99f422` | payload-create-1m | cleanup_ns | 2 | 291616708.5 | 285845500 | 297387917 |
| corrected / `39b70a103d99f422` | payload-create-1m | clone_bytes | 2 | 917504.0 | 917504 | 917504 |
| corrected / `39b70a103d99f422` | payload-create-1m | clone_wall_ns | 2 | 6699146.0 | 5863667 | 7534625 |
| corrected / `39b70a103d99f422` | payload-create-1m | command_wall_ns | 2 | 889753729.5 | 880029500 | 899477959 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_diagnostics.cdc_bytes_scanned | 2 | 1048576.0 | 1048576 | 1048576 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_diagnostics.edit_count | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_diagnostics.edit_metric_nodes_scanned | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_diagnostics.edit_piece_count | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_diagnostics.edit_piece_height | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_diagnostics.edit_piece_logical_charge | 2 | 8.0 | 8 | 8 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_diagnostics.edit_spool_allocated_bytes | 2 | 1048576.0 | 1048576 | 1048576 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_diagnostics.edit_spool_live_bytes | 2 | 1048576.0 | 1048576 | 1048576 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_diagnostics.edit_spool_peak_bytes | 2 | 1048576.0 | 1048576 | 1048576 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_diagnostics.edit_spool_superseded_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_diagnostics.edit_tree_visits | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_diagnostics.namespace_base_paths_visited | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_diagnostics.namespace_candidate_probe_nodes | 2 | 3.5 | 3 | 4 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_diagnostics.namespace_clean_nodes_visited | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_diagnostics.namespace_dirty_nodes_visited | 2 | 2.0 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_diagnostics.namespace_final_paths_visited | 2 | 7.0 | 7 | 7 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_diagnostics.physical_spool_allocated_bytes | 2 | 1048576.0 | 1048576 | 1048576 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_diagnostics.physical_spool_observation_count | 2 | 6.0 | 6 | 6 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_diagnostics.physical_spool_observation_errors | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_diagnostics.physical_spool_peak_bytes | 2 | 1048576.0 | 1048576 | 1048576 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_ns | 2 | 3640250.0 | 3295083 | 3985417 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.candidate_finish_ns | 2 | 34791.5 | 34583 | 35000 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.candidate_plan_ns | 2 | 1291.5 | 1291 | 1292 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.capture_ns | 2 | 9333.0 | 9208 | 9458 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.captured_bytes | 2 | 1048576.0 | 1048576 | 1048576 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.captured_files | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.content_ns | 2 | 80833.5 | 64458 | 97209 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.dirty_compare_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.in_place_rebase_ns | 2 | 900979.0 | 894750 | 907208 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.local_admission_ns | 2 | 115438.0 | 110709 | 120167 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.max_admission_transaction_bytes | 2 | 1053054.0 | 1053054 | 1053054 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.max_admission_transaction_objects | 2 | 66.0 | 66 | 66 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.namespace_ns | 2 | 20979.0 | 19708 | 22250 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.object_admission_begin_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.object_admission_commit_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.object_admission_insert_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.object_admission_ns | 2 | 125438.0 | 124917 | 125959 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.object_admission_transactions | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.pause_fence_ns | 2 | 487500.0 | 365500 | 609500 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.payload_bytes_read | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.publication_begin_ns | 2 | 3667.0 | 3542 | 3792 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.publication_commit_ns | 2 | 298521.0 | 296334 | 300708 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.publication_insert_ns | 2 | 579649.0 | 569000 | 590298 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.publication_metadata_ns | 2 | 73229.0 | 73083 | 73375 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.publication_ns | 2 | 962521.0 | 953708 | 971334 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.publication_payload_ns | 2 | 1354.0 | 1331 | 1377 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.quiesce_ns | 2 | 354.0 | 292 | 416 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.resume_ns | 2 | 328541.5 | 253291 | 403792 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.snapshot_database_bytes | 2 | 1140.0 | 1140 | 1140 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.snapshot_database_calls | 2 | 11.0 | 11 | 11 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.snapshot_database_rows | 2 | 11.0 | 11 | 11 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.total_ns | 2 | 3636958.5 | 3291792 | 3982125 |
| corrected / `39b70a103d99f422` | payload-create-1m | commit_work.unattributed_ns | 2 | 568958.5 | 430877 | 707040 |
| corrected / `39b70a103d99f422` | payload-create-1m | completed_chain_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | completed_episode_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | completed_file_write_count | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | completed_read_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | completed_read_request_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | completed_syscall_count | 2 | 7.0 | 7 | 7 |
| corrected / `39b70a103d99f422` | payload-create-1m | completed_target_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | completed_write_bytes | 2 | 1048576.0 | 1048576 | 1048576 |
| corrected / `39b70a103d99f422` | payload-create-1m | create_ns | 2 | 8907542.0 | 8406334 | 9408750 |
| corrected / `39b70a103d99f422` | payload-create-1m | created_commit_count | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | directory_entry_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | editor_save_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | end_ns | 2 | 3494541.5 | 2935125 | 4053958 |
| corrected / `39b70a103d99f422` | payload-create-1m | exec_ns | 2 | 11375291.5 | 11358750 | 11391833 |
| corrected / `39b70a103d99f422` | payload-create-1m | external_process_wall_ns | 2 | 107974354.0 | 107028000 | 108920708 |
| corrected / `39b70a103d99f422` | payload-create-1m | file_sync_ns | 2 | 1573916.5 | 1457958 | 1689875 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.callback_access | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.callback_create | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.callback_flush | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.callback_fsync | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.callback_fsyncdir | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.callback_getattr | 2 | 2.0 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.callback_link | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.callback_lookup | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.callback_mkdir | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.callback_mknod | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.callback_open | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.callback_opendir | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.callback_read | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.callback_readdir | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.callback_readdirplus | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.callback_readlink | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.callback_release | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.callback_releasedir | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.callback_rename | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.callback_rmdir | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.callback_setattr | 2 | 4.0 | 4 | 4 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.callback_statfs | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.callback_symlink | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.callback_unlink | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.callback_write | 2 | 2.0 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.client_decode_copy_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.client_decode_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.client_response_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.client_response_frames | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.client_socket_read_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.collection_ns | 2 | 715125.5 | 450209 | 980042 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.directory_entries_returned | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.directory_nonzero_offset_requests | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.host_dispatch_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.host_encode_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.host_response_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.host_response_copy_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.host_response_frames | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.host_socket_write_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.init_capabilities | 2 | 4481057.0 | 4481057 | 4481057 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.kernel_read_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.kernel_read_gt_1m | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.kernel_read_le_1m | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.kernel_read_le_256k | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.kernel_read_le_4k | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.kernel_read_le_64k | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.kernel_read_requests | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.local_bytes | 2 | 4807.0 | 4807 | 4807 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.local_calls | 2 | 52.0 | 52 | 52 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.local_ids | 2 | 52.0 | 52 | 52 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.local_read_auth_ns | 2 | 343874.0 | 342417 | 345331 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.local_rows | 2 | 52.0 | 52 | 52 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.max_payload_batch | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.max_readahead_bytes | 2 | 131072.0 | 131072 | 131072 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.payload_batches | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.payload_bytes_read | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.payload_ids | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.read_ahead_cache_copy_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.read_ahead_fetched_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.read_ahead_fetches | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.read_ahead_hits | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.read_ahead_misses | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.read_ahead_requested_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.read_ahead_served_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.read_ahead_unused_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.read_plan_builds | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.rope_nodes_read | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.snapshot_cache_bytes | 2 | 3525.0 | 3525 | 3525 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.snapshot_cache_hits | 2 | 39.0 | 39 | 39 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.snapshot_cache_rows | 2 | 39.0 | 39 | 39 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.snapshot_database_bytes | 2 | 1282.0 | 1282 | 1282 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.snapshot_database_calls | 2 | 13.0 | 13 | 13 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.snapshot_database_rows | 2 | 13.0 | 13 | 13 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.workspace_output_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.workspace_read_calls | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.workspace_read_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_read.workspace_requested_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_write.client_frame_bytes | 2 | 1048601.0 | 1048601 | 1048601 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_write.client_request_copy_bytes | 2 | 1048576.0 | 1048576 | 1048576 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_write.collection_ns | 2 | 459916.5 | 335083 | 584750 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_write.decode_ns | 2 | 12624.5 | 12583 | 12666 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_write.encode_ns | 2 | 62.5 | 42 | 83 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_write.frame_payload_copy_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_write.host_decode_copy_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_write.host_dispatch_ns | 2 | 308729.5 | 274917 | 342542 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_write.host_frame_bytes | 2 | 1048601.0 | 1048601 | 1048601 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_write.kernel_write_bytes | 2 | 1048576.0 | 1048576 | 1048576 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_write.kernel_write_gt_1m | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_write.kernel_write_le_1m | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_write.kernel_write_le_256k | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_write.kernel_write_le_4k | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_write.kernel_write_le_64k | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_write.kernel_write_requests | 2 | 2.0 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_write.max_write_bytes | 2 | 1048576.0 | 1048576 | 1048576 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_write.socket_read_ns | 2 | 2617457.5 | 2510999 | 2723916 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_write.socket_write_ns | 2 | 176479.5 | 173709 | 179250 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_write.spool_write_bytes | 2 | 1048576.0 | 1048576 | 1048576 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_write.spool_write_ns | 2 | 290354.0 | 289791 | 290917 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_write.spool_write_open_count | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_write.workspace_fence_count | 2 | 2.0 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-1m | fuse_write.workspace_fence_ns | 2 | 763770.5 | 482083 | 1045458 |
| corrected / `39b70a103d99f422` | payload-create-1m | git_process_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | host.disk_read_bytes.delta | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | host.disk_read_bytes.end | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | host.disk_read_bytes.start | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | host.disk_write_bytes.delta | 2 | 1056768.0 | 1056768 | 1056768 |
| corrected / `39b70a103d99f422` | payload-create-1m | host.disk_write_bytes.end | 2 | 1056768.0 | 1056768 | 1056768 |
| corrected / `39b70a103d99f422` | payload-create-1m | host.disk_write_bytes.start | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | host.peak_resident_bytes.max | 2 | 14581760.0 | 14467072 | 14696448 |
| corrected / `39b70a103d99f422` | payload-create-1m | host.physical_footprint_bytes.max | 2 | 9118140.0 | 8995248 | 9241032 |
| corrected / `39b70a103d99f422` | payload-create-1m | host.resident_bytes.max | 2 | 14581760.0 | 14467072 | 14696448 |
| corrected / `39b70a103d99f422` | payload-create-1m | host.swaps.delta | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | host.swaps.end | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | host.swaps.start | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | host.system_cpu_ns.delta | 2 | 29215750.0 | 28925583 | 29505917 |
| corrected / `39b70a103d99f422` | payload-create-1m | host.system_cpu_ns.end | 2 | 30847562.0 | 30442791 | 31252333 |
| corrected / `39b70a103d99f422` | payload-create-1m | host.system_cpu_ns.start | 2 | 1631812.0 | 1517208 | 1746416 |
| corrected / `39b70a103d99f422` | payload-create-1m | host.user_cpu_ns.delta | 2 | 13117729.5 | 13050125 | 13185334 |
| corrected / `39b70a103d99f422` | payload-create-1m | host.user_cpu_ns.end | 2 | 14923541.5 | 14819833 | 15027250 |
| corrected / `39b70a103d99f422` | payload-create-1m | host.user_cpu_ns.start | 2 | 1805812.0 | 1769708 | 1841916 |
| corrected / `39b70a103d99f422` | payload-create-1m | host_orchestration_ns | 2 | 56942479.5 | 56462500 | 57422459 |
| corrected / `39b70a103d99f422` | payload-create-1m | host_sampler.baseline_bytes | 2 | 2605056.0 | 2605056 | 2605056 |
| corrected / `39b70a103d99f422` | payload-create-1m | host_sampler.final_bytes | 2 | 12271616.0 | 12156928 | 12386304 |
| corrected / `39b70a103d99f422` | payload-create-1m | host_sampler.maximum_gap_ns | 2 | 12526812.0 | 12518833 | 12534791 |
| corrected / `39b70a103d99f422` | payload-create-1m | host_sampler.sample_count | 2 | 8.5 | 8 | 9 |
| corrected / `39b70a103d99f422` | payload-create-1m | host_sampler.sampled_peak_bytes | 2 | 14540800.0 | 14417920 | 14663680 |
| corrected / `39b70a103d99f422` | payload-create-1m | inplace_edit_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | input.fixture_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | input.regular_files | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | interrupted_syscall_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | lifecycle.anchor_prefetch_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | lifecycle.cleanup_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | lifecycle.docker_calls | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | lifecycle.docker_setup_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | lifecycle.helper_copy_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | lifecycle.mount_ready_ns | 2 | 7824416.5 | 7374833 | 8274000 |
| corrected / `39b70a103d99f422` | payload-create-1m | lifecycle.proxy_ns | 2 | 163875.0 | 158583 | 169167 |
| corrected / `39b70a103d99f422` | payload-create-1m | lifecycle.small_file_prefetch_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | lifecycle.small_file_prefetch_eligible | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | lifecycle.snapshot_cache_bytes_at_create | 2 | 1548.0 | 1548 | 1548 |
| corrected / `39b70a103d99f422` | payload-create-1m | lifecycle.snapshot_cache_rows_at_create | 2 | 10.0 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-create-1m | lifecycle.snapshot_database_bytes | 2 | 908.0 | 908 | 908 |
| corrected / `39b70a103d99f422` | payload-create-1m | lifecycle.snapshot_database_calls | 2 | 10.0 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-create-1m | lifecycle.snapshot_database_rows | 2 | 10.0 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-create-1m | lifecycle.snapshot_store_wide_scans | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | lifecycle.total_ns | 2 | 10040521.0 | 9884292 | 10196750 |
| corrected / `39b70a103d99f422` | payload-create-1m | lifecycle.unattributed_ns | 2 | 234271.5 | 230625 | 237918 |
| corrected / `39b70a103d99f422` | payload-create-1m | lifecycle.unmount_ns | 2 | 1059583.5 | 815417 | 1303750 |
| corrected / `39b70a103d99f422` | payload-create-1m | lifecycle.wait_ns | 2 | 758374.5 | 707541 | 809208 |
| corrected / `39b70a103d99f422` | payload-create-1m | metadata_normalization_count | 2 | 2.0 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-1m | metadata_normalization_ns | 2 | 653250.5 | 637459 | 669042 |
| corrected / `39b70a103d99f422` | payload-create-1m | orchestration_unattributed_ns | 2 | 29431021.0 | 29337584 | 29524458 |
| corrected / `39b70a103d99f422` | payload-create-1m | preparation_ns | 2 | 489224041.5 | 484377291 | 494070792 |
| corrected / `39b70a103d99f422` | payload-create-1m | pure_call_sum_ns | 2 | 27511458.5 | 27124916 | 27898001 |
| corrected / `39b70a103d99f422` | payload-create-1m | root_sync_ns | 2 | 339979.5 | 294542 | 385417 |
| corrected / `39b70a103d99f422` | payload-create-1m | runtime_preparation_ns | 2 | 385227646.0 | 382904583 | 387550709 |
| corrected / `39b70a103d99f422` | payload-create-1m | spool_boundary.max_allocated_bytes | 2 | 1052672.0 | 1052672 | 1052672 |
| corrected / `39b70a103d99f422` | payload-create-1m | spool_boundary.max_file_count | 2 | 2.0 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-1m | spool_boundary.max_logical_bytes | 2 | 1048721.0 | 1048721 | 1048721 |
| corrected / `39b70a103d99f422` | payload-create-1m | store.allocated_bytes.delta | 2 | 2293760.0 | 2293760 | 2293760 |
| corrected / `39b70a103d99f422` | payload-create-1m | store.allocated_bytes.end | 2 | 3211264.0 | 3211264 | 3211264 |
| corrected / `39b70a103d99f422` | payload-create-1m | store.allocated_bytes.max | 2 | 3211264.0 | 3211264 | 3211264 |
| corrected / `39b70a103d99f422` | payload-create-1m | store.allocated_bytes.start | 2 | 917504.0 | 917504 | 917504 |
| corrected / `39b70a103d99f422` | payload-create-1m | store.file_bytes.delta | 2 | 1310720.0 | 1310720 | 1310720 |
| corrected / `39b70a103d99f422` | payload-create-1m | store.file_bytes.end | 2 | 2228224.0 | 2228224 | 2228224 |
| corrected / `39b70a103d99f422` | payload-create-1m | store.file_bytes.max | 2 | 2228224.0 | 2228224 | 2228224 |
| corrected / `39b70a103d99f422` | payload-create-1m | store.file_bytes.start | 2 | 917504.0 | 917504 | 917504 |
| corrected / `39b70a103d99f422` | payload-create-1m | store.freelist_page_count.delta | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | store.freelist_page_count.end | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | store.freelist_page_count.max | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | store.freelist_page_count.start | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | store.live_page_bytes.delta | 2 | 1310720.0 | 1310720 | 1310720 |
| corrected / `39b70a103d99f422` | payload-create-1m | store.live_page_bytes.end | 2 | 2228224.0 | 2228224 | 2228224 |
| corrected / `39b70a103d99f422` | payload-create-1m | store.live_page_bytes.max | 2 | 2228224.0 | 2228224 | 2228224 |
| corrected / `39b70a103d99f422` | payload-create-1m | store.live_page_bytes.start | 2 | 917504.0 | 917504 | 917504 |
| corrected / `39b70a103d99f422` | payload-create-1m | store.page_count.delta | 2 | 20.0 | 20 | 20 |
| corrected / `39b70a103d99f422` | payload-create-1m | store.page_count.end | 2 | 34.0 | 34 | 34 |
| corrected / `39b70a103d99f422` | payload-create-1m | store.page_count.max | 2 | 34.0 | 34 | 34 |
| corrected / `39b70a103d99f422` | payload-create-1m | store.page_count.start | 2 | 14.0 | 14 | 14 |
| corrected / `39b70a103d99f422` | payload-create-1m | verified.canonical-verification.canonical_Chunk_bytes | 2 | 1049793.0 | 1049793 | 1049793 |
| corrected / `39b70a103d99f422` | payload-create-1m | verified.canonical-verification.canonical_Chunk_objects | 2 | 57.0 | 57 | 57 |
| corrected / `39b70a103d99f422` | payload-create-1m | verified.canonical-verification.canonical_DirectoryNode_bytes | 2 | 89.0 | 89 | 89 |
| corrected / `39b70a103d99f422` | payload-create-1m | verified.canonical-verification.canonical_DirectoryNode_objects | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | verified.canonical-verification.canonical_DirectoryState_bytes | 2 | 98.0 | 98 | 98 |
| corrected / `39b70a103d99f422` | payload-create-1m | verified.canonical-verification.canonical_DirectoryState_objects | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | verified.canonical-verification.canonical_FileNode_bytes | 2 | 2456.0 | 2456 | 2456 |
| corrected / `39b70a103d99f422` | payload-create-1m | verified.canonical-verification.canonical_FileNode_objects | 2 | 4.0 | 4 | 4 |
| corrected / `39b70a103d99f422` | payload-create-1m | verified.canonical-verification.canonical_FileState_bytes | 2 | 424.0 | 424 | 424 |
| corrected / `39b70a103d99f422` | payload-create-1m | verified.canonical-verification.canonical_FileState_objects | 2 | 4.0 | 4 | 4 |
| corrected / `39b70a103d99f422` | payload-create-1m | verified.canonical-verification.canonical_InodeRecord_bytes | 2 | 196.0 | 196 | 196 |
| corrected / `39b70a103d99f422` | payload-create-1m | verified.canonical-verification.canonical_InodeRecord_objects | 2 | 2.0 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-1m | verified.canonical-verification.canonical_InodeTable_bytes | 2 | 172.0 | 172 | 172 |
| corrected / `39b70a103d99f422` | payload-create-1m | verified.canonical-verification.canonical_InodeTable_objects | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | verified.canonical-verification.canonical_Metadata_bytes | 2 | 286.0 | 286 | 286 |
| corrected / `39b70a103d99f422` | payload-create-1m | verified.canonical-verification.canonical_Metadata_objects | 2 | 2.0 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-1m | verified.canonical-verification.canonical_Namespace_bytes | 2 | 121.0 | 121 | 121 |
| corrected / `39b70a103d99f422` | payload-create-1m | verified.canonical-verification.canonical_Namespace_objects | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | verified.canonical-verification.canonical_unique_bytes | 2 | 1053635.0 | 1053635 | 1053635 |
| corrected / `39b70a103d99f422` | payload-create-1m | verified.canonical-verification.canonical_unique_objects | 2 | 73.0 | 73 | 73 |
| corrected / `39b70a103d99f422` | payload-create-1m | verified.canonical-verification.independent_content_paths | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | verified.canonical-verification.logical_bytes | 2 | 1048576.0 | 1048576 | 1048576 |
| corrected / `39b70a103d99f422` | payload-create-1m | verified.canonical-verification.persistence_custody_paths | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | verified.canonical-verification.verified_paths | 2 | 2.0 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-1m | verified.canonical-verification.verified_regular_paths | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | visibility_ns | 2 | 93833.5 | 93542 | 94125 |
| corrected / `39b70a103d99f422` | payload-create-1m | visited_file_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | visited_path_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | workload_chmod_call_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | workload_close_call_count | 2 | 2.0 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-1m | workload_closedir_call_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | workload_fsync_call_count | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | workload_fsyncdir_call_count | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | workload_ftruncate_call_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | workload_lstat_call_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | workload_mkdir_call_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | workload_ns | 2 | 7212812.5 | 6814792 | 7610833 |
| corrected / `39b70a103d99f422` | payload-create-1m | workload_open_call_count | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | workload_open_directory_call_count | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | workload_opendir_call_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | workload_plan_ns | 2 | 146.0 | 125 | 167 |
| corrected / `39b70a103d99f422` | payload-create-1m | workload_pread_call_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | workload_pwrite_call_count | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-1m | workload_rename_call_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | workload_rmdir_call_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | workload_symlink_call_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-1m | workload_unlink_call_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | attempted_syscall_count | 3 | 506 | 506 | 506 |
| corrected / `39b70a103d99f422` | payload-create-500m | benchmark_injection_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | benchmark_reopen_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | benchmark_verifier_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | cache_acquisition_ns | 3 | 11667000 | 11547250 | 11933875 |
| corrected / `39b70a103d99f422` | payload-create-500m | cache_build_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | cache_validation_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | candidate.admission_transactions | 3 | 214 | 214 | 214 |
| corrected / `39b70a103d99f422` | payload-create-500m | candidate.batch_inserted_bytes | 3 | 525451627 | 525413935 | 525451627 |
| corrected / `39b70a103d99f422` | payload-create-500m | candidate.batch_inserted_objects | 3 | 27178 | 27178 | 27178 |
| corrected / `39b70a103d99f422` | payload-create-500m | candidate.candidate_bytes | 3 | 525955698 | 525955698 | 525955698 |
| corrected / `39b70a103d99f422` | payload-create-500m | candidate.candidate_objects | 3 | 27223 | 27223 | 27223 |
| corrected / `39b70a103d99f422` | payload-create-500m | candidate.final_inserted_bytes | 3 | 503848 | 503848 | 541540 |
| corrected / `39b70a103d99f422` | payload-create-500m | candidate.final_inserted_objects | 3 | 42 | 42 | 42 |
| corrected / `39b70a103d99f422` | payload-create-500m | candidate.inserted_bytes | 3 | 525955475 | 525955475 | 525955475 |
| corrected / `39b70a103d99f422` | payload-create-500m | candidate.inserted_objects | 3 | 27220 | 27220 | 27220 |
| corrected / `39b70a103d99f422` | payload-create-500m | candidate.max_transaction_bytes | 3 | 2633681 | 2633681 | 2639184 |
| corrected / `39b70a103d99f422` | payload-create-500m | candidate.max_transaction_objects | 3 | 127 | 127 | 127 |
| corrected / `39b70a103d99f422` | payload-create-500m | candidate.preexisting_reused_bytes | 3 | 223 | 223 | 223 |
| corrected / `39b70a103d99f422` | payload-create-500m | candidate.preexisting_reused_objects | 3 | 3 | 3 | 3 |
| corrected / `39b70a103d99f422` | payload-create-500m | candidate.reused_bytes | 3 | 223 | 223 | 223 |
| corrected / `39b70a103d99f422` | payload-create-500m | candidate.reused_objects | 3 | 3 | 3 | 3 |
| corrected / `39b70a103d99f422` | payload-create-500m | cgroup.cpu_usage_usec_delta | 3 | 442520 | 392717 | 509656 |
| corrected / `39b70a103d99f422` | payload-create-500m | cgroup.cpu_usage_usec_end | 3 | 486508 | 434911 | 552878 |
| corrected / `39b70a103d99f422` | payload-create-500m | cgroup.cpu_usage_usec_start | 3 | 43222 | 42194 | 43988 |
| corrected / `39b70a103d99f422` | payload-create-500m | cgroup.observed_max.memory.current | 3 | 10113024 | 9998336 | 10190848 |
| corrected / `39b70a103d99f422` | payload-create-500m | cgroup.observed_max.memory.events.oom | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | cgroup.observed_max.memory.events.oom_kill | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | cgroup.observed_max.memory.peak | 3 | 10366976 | 10219520 | 10653696 |
| corrected / `39b70a103d99f422` | payload-create-500m | cgroup.observed_max.memory.stat.anon | 3 | 4071424 | 4067328 | 4075520 |
| corrected / `39b70a103d99f422` | payload-create-500m | cgroup.observed_max.memory.stat.file | 3 | 4096 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | payload-create-500m | cgroup.observed_max.memory.stat.file_dirty | 3 | 4096 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | payload-create-500m | cgroup.observed_max.memory.stat.file_writeback | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | cgroup.observed_max.memory.stat.kernel | 3 | 1306624 | 1302528 | 1318912 |
| corrected / `39b70a103d99f422` | payload-create-500m | cgroup.observed_max.memory.stat.shmem | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | cgroup.observed_max.memory.stat.slab | 3 | 671288 | 670632 | 672400 |
| corrected / `39b70a103d99f422` | payload-create-500m | cgroup.observed_max.memory.swap.current | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | cgroup.observed_max.pids.current | 3 | 17 | 17 | 17 |
| corrected / `39b70a103d99f422` | payload-create-500m | cleanup_ns | 3 | 370656375 | 301217209 | 370951708 |
| corrected / `39b70a103d99f422` | payload-create-500m | clone_bytes | 3 | 917504 | 917504 | 917504 |
| corrected / `39b70a103d99f422` | payload-create-500m | clone_wall_ns | 3 | 8738333 | 6258833 | 15716542 |
| corrected / `39b70a103d99f422` | payload-create-500m | command_wall_ns | 3 | 4865523833 | 3611525667 | 7780412167 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_diagnostics.cdc_bytes_scanned | 3 | 524288000 | 524288000 | 524288000 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_diagnostics.edit_count | 3 | 500 | 500 | 500 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_diagnostics.edit_metric_nodes_scanned | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_diagnostics.edit_piece_count | 3 | 500 | 500 | 500 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_diagnostics.edit_piece_height | 3 | 20 | 20 | 20 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_diagnostics.edit_piece_logical_charge | 3 | 64000 | 64000 | 64000 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_diagnostics.edit_spool_allocated_bytes | 3 | 524288000 | 524288000 | 524288000 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_diagnostics.edit_spool_live_bytes | 3 | 524288000 | 524288000 | 524288000 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_diagnostics.edit_spool_peak_bytes | 3 | 524288000 | 524288000 | 524288000 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_diagnostics.edit_spool_superseded_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_diagnostics.edit_tree_visits | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_diagnostics.namespace_base_paths_visited | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_diagnostics.namespace_candidate_probe_nodes | 3 | 4 | 3 | 4 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_diagnostics.namespace_clean_nodes_visited | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_diagnostics.namespace_dirty_nodes_visited | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_diagnostics.namespace_final_paths_visited | 3 | 7 | 7 | 7 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_diagnostics.physical_spool_allocated_bytes | 3 | 529530880 | 529530880 | 529530880 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_diagnostics.physical_spool_observation_count | 3 | 1503 | 1503 | 1503 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_diagnostics.physical_spool_observation_errors | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_diagnostics.physical_spool_peak_bytes | 3 | 529530880 | 529530880 | 529530880 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_ns | 3 | 2291853458 | 1441614666 | 2463810167 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.candidate_finish_ns | 3 | 18071833 | 17649709 | 20385334 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.candidate_plan_ns | 3 | 25302208 | 12028667 | 48660916 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.capture_ns | 3 | 8500 | 8459 | 8667 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.captured_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.captured_files | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.content_ns | 3 | 1642670625 | 883174083 | 1713838458 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.dirty_compare_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.in_place_rebase_ns | 3 | 19025709 | 10755333 | 96504084 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.local_admission_ns | 3 | 66229292 | 64592625 | 70959250 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.max_admission_transaction_bytes | 3 | 2633681 | 2633681 | 2639184 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.max_admission_transaction_objects | 3 | 127 | 127 | 127 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.namespace_ns | 3 | 27333 | 21666 | 27500 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.object_admission_begin_ns | 3 | 1449036 | 1294717 | 1784254 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.object_admission_commit_ns | 3 | 132536504 | 129520711 | 188063788 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.object_admission_insert_ns | 3 | 199937655 | 198843585 | 239451009 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.object_admission_ns | 3 | 441213583 | 440971708 | 569614333 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.object_admission_transactions | 3 | 214 | 214 | 214 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.pause_fence_ns | 3 | 341208 | 322291 | 372791 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.payload_bytes_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.publication_begin_ns | 3 | 4292 | 4125 | 7167 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.publication_commit_ns | 3 | 260750 | 251792 | 357084 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.publication_insert_ns | 3 | 361623 | 341121 | 461836 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.publication_metadata_ns | 3 | 139041 | 123209 | 175584 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.publication_ns | 3 | 770208 | 724542 | 1007042 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.publication_payload_ns | 3 | 1121 | 958 | 1122 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.quiesce_ns | 3 | 292 | 291 | 417 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.resume_ns | 3 | 587292 | 537250 | 1162166 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.snapshot_database_bytes | 3 | 1140 | 1140 | 1140 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.snapshot_database_calls | 3 | 11 | 11 | 11 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.snapshot_database_rows | 3 | 11 | 11 | 11 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.total_ns | 3 | 2291845542 | 1441610000 | 2463805250 |
| corrected / `39b70a103d99f422` | payload-create-500m | commit_work.unattributed_ns | 3 | 8459585 | 8167668 | 13057874 |
| corrected / `39b70a103d99f422` | payload-create-500m | completed_chain_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | completed_episode_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | completed_file_write_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-500m | completed_read_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | completed_read_request_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | completed_syscall_count | 3 | 506 | 506 | 506 |
| corrected / `39b70a103d99f422` | payload-create-500m | completed_target_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | completed_write_bytes | 3 | 524288000 | 524288000 | 524288000 |
| corrected / `39b70a103d99f422` | payload-create-500m | create_ns | 3 | 11837459 | 9346125 | 20990291 |
| corrected / `39b70a103d99f422` | payload-create-500m | created_commit_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-500m | directory_entry_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | editor_save_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | end_ns | 3 | 3240208 | 3089209 | 5128958 |
| corrected / `39b70a103d99f422` | payload-create-500m | exec_ns | 3 | 1428129417 | 1173164750 | 4203255125 |
| corrected / `39b70a103d99f422` | payload-create-500m | external_process_wall_ns | 3 | 3922577666 | 2824895334 | 6873984750 |
| corrected / `39b70a103d99f422` | payload-create-500m | file_sync_ns | 3 | 2598584 | 2136083 | 2678333 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.callback_access | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.callback_create | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.callback_flush | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.callback_fsync | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.callback_fsyncdir | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.callback_getattr | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.callback_link | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.callback_lookup | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.callback_mkdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.callback_mknod | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.callback_open | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.callback_opendir | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.callback_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.callback_readdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.callback_readdirplus | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.callback_readlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.callback_release | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.callback_releasedir | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.callback_rename | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.callback_rmdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.callback_setattr | 3 | 4 | 4 | 4 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.callback_statfs | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.callback_symlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.callback_unlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.callback_write | 3 | 1000 | 1000 | 1000 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.client_decode_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.client_decode_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.client_response_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.client_response_frames | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.client_socket_read_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.collection_ns | 3 | 528667 | 427334 | 1000917 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.directory_entries_returned | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.directory_nonzero_offset_requests | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.host_dispatch_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.host_encode_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.host_response_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.host_response_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.host_response_frames | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.host_socket_write_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.init_capabilities | 3 | 4481057 | 4481057 | 4481057 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.kernel_read_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.kernel_read_gt_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.kernel_read_le_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.kernel_read_le_256k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.kernel_read_le_4k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.kernel_read_le_64k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.kernel_read_requests | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.local_bytes | 3 | 4807 | 4807 | 4807 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.local_calls | 3 | 52 | 52 | 52 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.local_ids | 3 | 52 | 52 | 52 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.local_read_auth_ns | 3 | 453874 | 452713 | 455371 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.local_rows | 3 | 52 | 52 | 52 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.max_payload_batch | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.max_readahead_bytes | 3 | 131072 | 131072 | 131072 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.payload_batches | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.payload_bytes_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.payload_ids | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.read_ahead_cache_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.read_ahead_fetched_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.read_ahead_fetches | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.read_ahead_hits | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.read_ahead_misses | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.read_ahead_requested_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.read_ahead_served_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.read_ahead_unused_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.read_plan_builds | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.rope_nodes_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.snapshot_cache_bytes | 3 | 3525 | 3525 | 3525 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.snapshot_cache_hits | 3 | 39 | 39 | 39 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.snapshot_cache_rows | 3 | 39 | 39 | 39 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.snapshot_database_bytes | 3 | 1282 | 1282 | 1282 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.snapshot_database_calls | 3 | 13 | 13 | 13 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.snapshot_database_rows | 3 | 13 | 13 | 13 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.workspace_output_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.workspace_read_calls | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.workspace_read_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_read.workspace_requested_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_write.client_frame_bytes | 3 | 524300500 | 524300500 | 524300500 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_write.client_request_copy_bytes | 3 | 524288000 | 524288000 | 524288000 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_write.collection_ns | 3 | 527292 | 302875 | 538875 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_write.decode_ns | 3 | 10715423 | 10536039 | 10827382 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_write.encode_ns | 3 | 26968 | 23066 | 27341 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_write.frame_payload_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_write.host_decode_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_write.host_dispatch_ns | 3 | 1339233668 | 1076322339 | 4095150534 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_write.host_frame_bytes | 3 | 524300500 | 524300500 | 524300500 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_write.kernel_write_bytes | 3 | 524288000 | 524288000 | 524288000 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_write.kernel_write_gt_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_write.kernel_write_le_1m | 3 | 500 | 500 | 500 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_write.kernel_write_le_256k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_write.kernel_write_le_4k | 3 | 500 | 500 | 500 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_write.kernel_write_le_64k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_write.kernel_write_requests | 3 | 1000 | 1000 | 1000 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_write.max_write_bytes | 3 | 1048576 | 1048576 | 1048576 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_write.socket_read_ns | 3 | 73472385 | 68227829 | 86334160 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_write.socket_write_ns | 3 | 971101208 | 714131360 | 3609373466 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_write.spool_write_bytes | 3 | 524288000 | 524288000 | 524288000 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_write.spool_write_ns | 3 | 463766413 | 270228038 | 2654980205 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_write.spool_write_open_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_write.workspace_fence_count | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-500m | fuse_write.workspace_fence_ns | 3 | 2163209 | 1767959 | 2262458 |
| corrected / `39b70a103d99f422` | payload-create-500m | git_process_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | host.disk_read_bytes.delta | 3 | 2400256 | 1593344 | 30027776 |
| corrected / `39b70a103d99f422` | payload-create-500m | host.disk_read_bytes.end | 3 | 2400256 | 1593344 | 30027776 |
| corrected / `39b70a103d99f422` | payload-create-500m | host.disk_read_bytes.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | host.disk_write_bytes.delta | 3 | 1580113920 | 1580109824 | 1580118016 |
| corrected / `39b70a103d99f422` | payload-create-500m | host.disk_write_bytes.end | 3 | 1580113920 | 1580109824 | 1580118016 |
| corrected / `39b70a103d99f422` | payload-create-500m | host.disk_write_bytes.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | host.peak_resident_bytes.max | 3 | 89243648 | 85950464 | 89604096 |
| corrected / `39b70a103d99f422` | payload-create-500m | host.physical_footprint_bytes.max | 3 | 44302840 | 41042472 | 44696104 |
| corrected / `39b70a103d99f422` | payload-create-500m | host.resident_bytes.max | 3 | 89210880 | 85917696 | 89571328 |
| corrected / `39b70a103d99f422` | payload-create-500m | host.swaps.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | host.swaps.end | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | host.swaps.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | host.system_cpu_ns.delta | 3 | 764484791 | 724488208 | 1266810000 |
| corrected / `39b70a103d99f422` | payload-create-500m | host.system_cpu_ns.end | 3 | 766022541 | 726190583 | 1268480708 |
| corrected / `39b70a103d99f422` | payload-create-500m | host.system_cpu_ns.start | 3 | 1670708 | 1537750 | 1702375 |
| corrected / `39b70a103d99f422` | payload-create-500m | host.user_cpu_ns.delta | 3 | 1817436333 | 1807799750 | 1880375042 |
| corrected / `39b70a103d99f422` | payload-create-500m | host.user_cpu_ns.end | 3 | 1819279166 | 1809647916 | 1882274583 |
| corrected / `39b70a103d99f422` | payload-create-500m | host.user_cpu_ns.start | 3 | 1848166 | 1842833 | 1899541 |
| corrected / `39b70a103d99f422` | payload-create-500m | host_orchestration_ns | 3 | 3799056167 | 2663613708 | 6713924500 |
| corrected / `39b70a103d99f422` | payload-create-500m | host_sampler.baseline_bytes | 3 | 2588672 | 2588672 | 2605056 |
| corrected / `39b70a103d99f422` | payload-create-500m | host_sampler.final_bytes | 3 | 47415296 | 44154880 | 47775744 |
| corrected / `39b70a103d99f422` | payload-create-500m | host_sampler.maximum_gap_ns | 3 | 12588833 | 12588791 | 13358625 |
| corrected / `39b70a103d99f422` | payload-create-500m | host_sampler.sample_count | 3 | 347 | 248 | 592 |
| corrected / `39b70a103d99f422` | payload-create-500m | host_sampler.sampled_peak_bytes | 3 | 89210880 | 85934080 | 89587712 |
| corrected / `39b70a103d99f422` | payload-create-500m | inplace_edit_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | input.fixture_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | input.regular_files | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | interrupted_syscall_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | lifecycle.anchor_prefetch_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | lifecycle.cleanup_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | lifecycle.docker_calls | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | lifecycle.docker_setup_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | lifecycle.helper_copy_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | lifecycle.mount_ready_ns | 3 | 10811375 | 8284000 | 19892750 |
| corrected / `39b70a103d99f422` | payload-create-500m | lifecycle.proxy_ns | 3 | 170209 | 163750 | 171958 |
| corrected / `39b70a103d99f422` | payload-create-500m | lifecycle.small_file_prefetch_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | lifecycle.small_file_prefetch_eligible | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | lifecycle.snapshot_cache_bytes_at_create | 3 | 1548 | 1548 | 1548 |
| corrected / `39b70a103d99f422` | payload-create-500m | lifecycle.snapshot_cache_rows_at_create | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-create-500m | lifecycle.snapshot_database_bytes | 3 | 908 | 908 | 908 |
| corrected / `39b70a103d99f422` | payload-create-500m | lifecycle.snapshot_database_calls | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-create-500m | lifecycle.snapshot_database_rows | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-create-500m | lifecycle.snapshot_store_wide_scans | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | lifecycle.total_ns | 3 | 12696666 | 11025250 | 22022208 |
| corrected / `39b70a103d99f422` | payload-create-500m | lifecycle.unattributed_ns | 3 | 234834 | 224123 | 281667 |
| corrected / `39b70a103d99f422` | payload-create-500m | lifecycle.unmount_ns | 3 | 662500 | 661166 | 1009375 |
| corrected / `39b70a103d99f422` | payload-create-500m | lifecycle.wait_ns | 3 | 1022875 | 828459 | 1325083 |
| corrected / `39b70a103d99f422` | payload-create-500m | metadata_normalization_count | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-500m | metadata_normalization_ns | 3 | 793791 | 736041 | 924125 |
| corrected / `39b70a103d99f422` | payload-create-500m | orchestration_unattributed_ns | 3 | 33783665 | 32246666 | 54716543 |
| corrected / `39b70a103d99f422` | payload-create-500m | preparation_ns | 3 | 534451583 | 484484167 | 571352708 |
| corrected / `39b70a103d99f422` | payload-create-500m | pure_call_sum_ns | 3 | 3744339624 | 2629830043 | 6681677834 |
| corrected / `39b70a103d99f422` | payload-create-500m | root_sync_ns | 3 | 389708 | 386708 | 391500 |
| corrected / `39b70a103d99f422` | payload-create-500m | runtime_preparation_ns | 3 | 427138917 | 377403875 | 462095083 |
| corrected / `39b70a103d99f422` | payload-create-500m | spool_boundary.max_allocated_bytes | 3 | 529534976 | 529534976 | 529534976 |
| corrected / `39b70a103d99f422` | payload-create-500m | spool_boundary.max_file_count | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-500m | spool_boundary.max_logical_bytes | 3 | 524288147 | 524288147 | 524288147 |
| corrected / `39b70a103d99f422` | payload-create-500m | store.allocated_bytes.delta | 3 | 636682240 | 636682240 | 636682240 |
| corrected / `39b70a103d99f422` | payload-create-500m | store.allocated_bytes.end | 3 | 637599744 | 637599744 | 637599744 |
| corrected / `39b70a103d99f422` | payload-create-500m | store.allocated_bytes.max | 3 | 637599744 | 637599744 | 637599744 |
| corrected / `39b70a103d99f422` | payload-create-500m | store.allocated_bytes.start | 3 | 917504 | 917504 | 917504 |
| corrected / `39b70a103d99f422` | payload-create-500m | store.file_bytes.delta | 3 | 625213440 | 625147904 | 625213440 |
| corrected / `39b70a103d99f422` | payload-create-500m | store.file_bytes.end | 3 | 626130944 | 626065408 | 626130944 |
| corrected / `39b70a103d99f422` | payload-create-500m | store.file_bytes.max | 3 | 626130944 | 626065408 | 626130944 |
| corrected / `39b70a103d99f422` | payload-create-500m | store.file_bytes.start | 3 | 917504 | 917504 | 917504 |
| corrected / `39b70a103d99f422` | payload-create-500m | store.freelist_page_count.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | store.freelist_page_count.end | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | store.freelist_page_count.max | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | store.freelist_page_count.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | store.live_page_bytes.delta | 3 | 625213440 | 625147904 | 625213440 |
| corrected / `39b70a103d99f422` | payload-create-500m | store.live_page_bytes.end | 3 | 626130944 | 626065408 | 626130944 |
| corrected / `39b70a103d99f422` | payload-create-500m | store.live_page_bytes.max | 3 | 626130944 | 626065408 | 626130944 |
| corrected / `39b70a103d99f422` | payload-create-500m | store.live_page_bytes.start | 3 | 917504 | 917504 | 917504 |
| corrected / `39b70a103d99f422` | payload-create-500m | store.page_count.delta | 3 | 9540 | 9539 | 9540 |
| corrected / `39b70a103d99f422` | payload-create-500m | store.page_count.end | 3 | 9554 | 9553 | 9554 |
| corrected / `39b70a103d99f422` | payload-create-500m | store.page_count.max | 3 | 9554 | 9553 | 9554 |
| corrected / `39b70a103d99f422` | payload-create-500m | store.page_count.start | 3 | 14 | 14 | 14 |
| corrected / `39b70a103d99f422` | payload-create-500m | verified.canonical-verification.canonical_Chunk_bytes | 3 | 524854978 | 524854978 | 524854978 |
| corrected / `39b70a103d99f422` | payload-create-500m | verified.canonical-verification.canonical_Chunk_objects | 3 | 26998 | 26998 | 26998 |
| corrected / `39b70a103d99f422` | payload-create-500m | verified.canonical-verification.canonical_DirectoryNode_bytes | 3 | 89 | 89 | 89 |
| corrected / `39b70a103d99f422` | payload-create-500m | verified.canonical-verification.canonical_DirectoryNode_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-500m | verified.canonical-verification.canonical_DirectoryState_bytes | 3 | 98 | 98 | 98 |
| corrected / `39b70a103d99f422` | payload-create-500m | verified.canonical-verification.canonical_DirectoryState_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-500m | verified.canonical-verification.canonical_FileNode_bytes | 3 | 1099692 | 1099692 | 1099692 |
| corrected / `39b70a103d99f422` | payload-create-500m | verified.canonical-verification.canonical_FileNode_objects | 3 | 217 | 217 | 217 |
| corrected / `39b70a103d99f422` | payload-create-500m | verified.canonical-verification.canonical_FileState_bytes | 3 | 424 | 424 | 424 |
| corrected / `39b70a103d99f422` | payload-create-500m | verified.canonical-verification.canonical_FileState_objects | 3 | 4 | 4 | 4 |
| corrected / `39b70a103d99f422` | payload-create-500m | verified.canonical-verification.canonical_InodeRecord_bytes | 3 | 196 | 196 | 196 |
| corrected / `39b70a103d99f422` | payload-create-500m | verified.canonical-verification.canonical_InodeRecord_objects | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-500m | verified.canonical-verification.canonical_InodeTable_bytes | 3 | 172 | 172 | 172 |
| corrected / `39b70a103d99f422` | payload-create-500m | verified.canonical-verification.canonical_InodeTable_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-500m | verified.canonical-verification.canonical_Metadata_bytes | 3 | 286 | 286 | 286 |
| corrected / `39b70a103d99f422` | payload-create-500m | verified.canonical-verification.canonical_Metadata_objects | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-500m | verified.canonical-verification.canonical_Namespace_bytes | 3 | 121 | 121 | 121 |
| corrected / `39b70a103d99f422` | payload-create-500m | verified.canonical-verification.canonical_Namespace_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-500m | verified.canonical-verification.canonical_unique_bytes | 3 | 525956056 | 525956056 | 525956056 |
| corrected / `39b70a103d99f422` | payload-create-500m | verified.canonical-verification.canonical_unique_objects | 3 | 27227 | 27227 | 27227 |
| corrected / `39b70a103d99f422` | payload-create-500m | verified.canonical-verification.independent_content_paths | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-500m | verified.canonical-verification.logical_bytes | 3 | 524288000 | 524288000 | 524288000 |
| corrected / `39b70a103d99f422` | payload-create-500m | verified.canonical-verification.persistence_custody_paths | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | verified.canonical-verification.verified_paths | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-500m | verified.canonical-verification.verified_regular_paths | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-500m | visibility_ns | 3 | 126250 | 123959 | 137459 |
| corrected / `39b70a103d99f422` | payload-create-500m | visited_file_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | visited_path_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | workload_chmod_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | workload_close_call_count | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-create-500m | workload_closedir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | workload_fsync_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-500m | workload_fsyncdir_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-500m | workload_ftruncate_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | workload_lstat_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | workload_mkdir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | workload_ns | 3 | 1424233292 | 1166662543 | 4198148794 |
| corrected / `39b70a103d99f422` | payload-create-500m | workload_open_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-500m | workload_open_directory_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-create-500m | workload_opendir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | workload_plan_ns | 3 | 125 | 125 | 250 |
| corrected / `39b70a103d99f422` | payload-create-500m | workload_pread_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | workload_pwrite_call_count | 3 | 500 | 500 | 500 |
| corrected / `39b70a103d99f422` | payload-create-500m | workload_rename_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | workload_rmdir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | workload_symlink_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-create-500m | workload_unlink_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | attempted_syscall_count | 3 | 3 | 3 | 3 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | benchmark_injection_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | benchmark_reopen_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | benchmark_verifier_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | cache_acquisition_ns | 3 | 11372041 | 11285292 | 470913083 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | cache_build_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | cache_validation_ns | 3 | 0 | 0 | 466389166 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | cgroup.cpu_usage_usec_delta | 3 | 9816 | 9709 | 10244 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | cgroup.cpu_usage_usec_end | 3 | 57496 | 56400 | 58462 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | cgroup.cpu_usage_usec_start | 3 | 47787 | 46156 | 48646 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | cgroup.observed_max.memory.current | 3 | 4734976 | 4362240 | 4816896 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | cgroup.observed_max.memory.events.oom | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | cgroup.observed_max.memory.events.oom_kill | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | cgroup.observed_max.memory.peak | 3 | 5017600 | 5001216 | 5263360 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | cgroup.observed_max.memory.stat.anon | 3 | 2969600 | 2969600 | 2969600 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | cgroup.observed_max.memory.stat.file | 3 | 12288 | 12288 | 12288 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | cgroup.observed_max.memory.stat.file_dirty | 3 | 4096 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | cgroup.observed_max.memory.stat.file_writeback | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | cgroup.observed_max.memory.stat.kernel | 3 | 1277952 | 1277952 | 1306624 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | cgroup.observed_max.memory.stat.shmem | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | cgroup.observed_max.memory.stat.slab | 3 | 659376 | 650744 | 673784 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | cgroup.observed_max.memory.swap.current | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | cgroup.observed_max.pids.current | 3 | 17 | 12 | 17 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | cleanup_ns | 3 | 308774375 | 302786292 | 313437000 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | clone_bytes | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | clone_wall_ns | 3 | 467065375 | 444482958 | 467955083 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | command_wall_ns | 3 | 1384006875 | 1339776125 | 5874604209 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_diagnostics.cdc_bytes_scanned | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_diagnostics.edit_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_diagnostics.edit_metric_nodes_scanned | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_diagnostics.edit_piece_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_diagnostics.edit_piece_height | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_diagnostics.edit_piece_logical_charge | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_diagnostics.edit_spool_allocated_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_diagnostics.edit_spool_live_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_diagnostics.edit_spool_peak_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_diagnostics.edit_spool_superseded_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_diagnostics.edit_tree_visits | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_diagnostics.namespace_base_paths_visited | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_diagnostics.namespace_candidate_probe_nodes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_diagnostics.namespace_clean_nodes_visited | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_diagnostics.namespace_dirty_nodes_visited | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_diagnostics.namespace_final_paths_visited | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_diagnostics.physical_spool_allocated_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_diagnostics.physical_spool_observation_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_diagnostics.physical_spool_observation_errors | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_diagnostics.physical_spool_peak_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_ns | 3 | 1374708 | 1181000 | 1535042 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.candidate_finish_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.candidate_plan_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.capture_ns | 3 | 8792 | 7416 | 9042 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.captured_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.captured_files | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.content_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.dirty_compare_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.in_place_rebase_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.local_admission_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.max_admission_transaction_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.max_admission_transaction_objects | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.namespace_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.object_admission_begin_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.object_admission_commit_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.object_admission_insert_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.object_admission_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.object_admission_transactions | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.pause_fence_ns | 3 | 651708 | 517666 | 825750 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.payload_bytes_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.publication_begin_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.publication_commit_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.publication_insert_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.publication_metadata_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.publication_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.publication_payload_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.quiesce_ns | 3 | 250 | 250 | 500 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.resume_ns | 3 | 222500 | 199125 | 224917 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.snapshot_database_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.snapshot_database_calls | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.snapshot_database_rows | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.total_ns | 3 | 1371083 | 1178125 | 1531083 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | commit_work.unattributed_ns | 3 | 471374 | 451792 | 489209 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | completed_chain_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | completed_episode_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | completed_file_write_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | completed_read_bytes | 3 | 4096 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | completed_read_request_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | completed_syscall_count | 3 | 3 | 3 | 3 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | completed_target_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | completed_write_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | create_ns | 3 | 10710667 | 9265167 | 11425000 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | created_commit_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | directory_entry_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | editor_save_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | end_ns | 3 | 3886417 | 3028417 | 4132000 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | exec_ns | 3 | 14104750 | 11263833 | 15095125 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | external_process_wall_ns | 3 | 106456959 | 105748625 | 107752041 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.callback_access | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.callback_create | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.callback_flush | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.callback_fsync | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.callback_fsyncdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.callback_getattr | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.callback_link | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.callback_lookup | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.callback_mkdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.callback_mknod | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.callback_open | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.callback_opendir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.callback_read | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.callback_readdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.callback_readdirplus | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.callback_readlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.callback_release | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.callback_releasedir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.callback_rename | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.callback_rmdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.callback_setattr | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.callback_statfs | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.callback_symlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.callback_unlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.callback_write | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.client_decode_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.client_decode_ns | 3 | 14959 | 12459 | 24959 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.client_response_bytes | 3 | 2097161 | 2097161 | 2097161 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.client_response_frames | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.client_socket_read_ns | 3 | 7156166 | 6055585 | 7257875 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.collection_ns | 3 | 474958 | 466208 | 1013042 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.directory_entries_returned | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.directory_nonzero_offset_requests | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.host_dispatch_ns | 3 | 3682917 | 3608459 | 3743875 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.host_encode_ns | 3 | 0 | 0 | 42 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.host_response_bytes | 3 | 2097161 | 2097161 | 2097161 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.host_response_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.host_response_frames | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.host_socket_write_ns | 3 | 774375 | 574500 | 894916 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.init_capabilities | 3 | 4481057 | 4481057 | 4481057 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.kernel_read_bytes | 3 | 8192 | 8192 | 8192 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.kernel_read_gt_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.kernel_read_le_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.kernel_read_le_256k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.kernel_read_le_4k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.kernel_read_le_64k | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.kernel_read_requests | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.local_bytes | 3 | 2129108 | 2117847 | 2136742 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.local_calls | 3 | 18 | 18 | 19 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.local_ids | 3 | 125 | 123 | 129 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.local_read_auth_ns | 3 | 3748251 | 3687752 | 3852999 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.local_rows | 3 | 125 | 123 | 129 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.max_payload_batch | 3 | 108 | 106 | 111 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.max_readahead_bytes | 3 | 2097152 | 2097152 | 2097152 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.payload_batches | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.payload_bytes_read | 3 | 2097152 | 2097152 | 2097152 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.payload_ids | 3 | 108 | 106 | 111 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.read_ahead_cache_copy_bytes | 3 | 8192 | 8192 | 8192 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.read_ahead_fetched_bytes | 3 | 2097152 | 2097152 | 2097152 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.read_ahead_fetches | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.read_ahead_hits | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.read_ahead_misses | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.read_ahead_requested_bytes | 3 | 2097152 | 2097152 | 2097152 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.read_ahead_served_bytes | 3 | 8192 | 8192 | 8192 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.read_ahead_unused_bytes | 3 | 2088960 | 2088960 | 2088960 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.read_plan_builds | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.rope_nodes_read | 3 | 4 | 4 | 5 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.snapshot_cache_bytes | 3 | 644 | 644 | 644 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.snapshot_cache_hits | 3 | 6 | 6 | 6 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.snapshot_cache_rows | 3 | 6 | 6 | 6 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.snapshot_database_bytes | 3 | 2128464 | 2117203 | 2136098 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.snapshot_database_calls | 3 | 12 | 12 | 13 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.snapshot_database_rows | 3 | 119 | 117 | 123 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.workspace_output_bytes | 3 | 2097152 | 2097152 | 2097152 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.workspace_read_calls | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.workspace_read_ns | 3 | 3677709 | 3603584 | 3737875 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_read.workspace_requested_bytes | 3 | 2097152 | 2097152 | 2097152 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_write.client_frame_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_write.client_request_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_write.collection_ns | 3 | 395292 | 384042 | 422875 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_write.decode_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_write.encode_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_write.frame_payload_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_write.host_decode_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_write.host_dispatch_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_write.host_frame_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_write.kernel_write_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_write.kernel_write_gt_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_write.kernel_write_le_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_write.kernel_write_le_256k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_write.kernel_write_le_4k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_write.kernel_write_le_64k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_write.kernel_write_requests | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_write.max_write_bytes | 3 | 1048576 | 1048576 | 1048576 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_write.socket_read_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_write.socket_write_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_write.spool_write_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_write.spool_write_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_write.spool_write_open_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_write.workspace_fence_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | fuse_write.workspace_fence_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | git_process_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | host.disk_read_bytes.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | host.disk_read_bytes.end | 3 | 0 | 0 | 4096 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | host.disk_read_bytes.start | 3 | 0 | 0 | 4096 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | host.disk_write_bytes.delta | 3 | 8192 | 8192 | 8192 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | host.disk_write_bytes.end | 3 | 8192 | 8192 | 8192 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | host.disk_write_bytes.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | host.peak_resident_bytes.max | 3 | 17022976 | 16760832 | 17072128 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | host.physical_footprint_bytes.max | 3 | 8225272 | 8094128 | 8405448 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | host.resident_bytes.max | 3 | 17006592 | 16760832 | 17055744 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | host.swaps.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | host.swaps.end | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | host.swaps.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | host.system_cpu_ns.delta | 3 | 28395042 | 28128791 | 28973334 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | host.system_cpu_ns.end | 3 | 30647875 | 30220166 | 30924250 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | host.system_cpu_ns.start | 3 | 2091375 | 1950916 | 2252833 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | host.user_cpu_ns.delta | 3 | 13309208 | 13131916 | 13570541 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | host.user_cpu_ns.end | 3 | 15221791 | 15120041 | 15449541 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | host.user_cpu_ns.start | 3 | 1912583 | 1879000 | 1988125 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | host_orchestration_ns | 3 | 58607125 | 55453916 | 61567125 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | host_sampler.baseline_bytes | 3 | 2605056 | 2572288 | 2605056 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | host_sampler.final_bytes | 3 | 11108352 | 11010048 | 11321344 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | host_sampler.maximum_gap_ns | 3 | 12521083 | 11578792 | 12521834 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | host_sampler.sample_count | 3 | 9 | 9 | 9 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | host_sampler.sampled_peak_bytes | 3 | 17006592 | 16728064 | 17055744 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | inplace_edit_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | input.fixture_bytes | 3 | 524288000 | 524288000 | 524288000 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | input.regular_files | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | interrupted_syscall_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | lifecycle.anchor_prefetch_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | lifecycle.cleanup_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | lifecycle.docker_calls | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | lifecycle.docker_setup_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | lifecycle.helper_copy_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | lifecycle.mount_ready_ns | 3 | 9510917 | 7967542 | 10145750 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | lifecycle.proxy_ns | 3 | 162917 | 161625 | 189334 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | lifecycle.small_file_prefetch_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | lifecycle.small_file_prefetch_eligible | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | lifecycle.snapshot_cache_bytes_at_create | 3 | 1612 | 1612 | 1612 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | lifecycle.snapshot_cache_rows_at_create | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | lifecycle.snapshot_database_bytes | 3 | 972 | 972 | 972 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | lifecycle.snapshot_database_calls | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | lifecycle.snapshot_database_rows | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | lifecycle.snapshot_store_wide_scans | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | lifecycle.total_ns | 3 | 11483666 | 10242333 | 13175084 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | lifecycle.unattributed_ns | 3 | 246251 | 222039 | 334583 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | lifecycle.unmount_ns | 3 | 747959 | 680542 | 1383583 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | lifecycle.wait_ns | 3 | 1098041 | 839834 | 1210166 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | metadata_normalization_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | orchestration_unattributed_ns | 3 | 29412500 | 29166999 | 29662957 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | preparation_ns | 3 | 968616875 | 929670500 | 5452423792 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | pure_call_sum_ns | 3 | 28944168 | 26286917 | 32154625 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | runtime_preparation_ns | 3 | 396100250 | 391609083 | 402665708 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | spool_boundary.max_allocated_bytes | 3 | 4096 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | spool_boundary.max_file_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | spool_boundary.max_logical_bytes | 3 | 149 | 149 | 149 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | store.allocated_bytes.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | store.allocated_bytes.end | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | store.allocated_bytes.max | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | store.allocated_bytes.start | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | store.file_bytes.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | store.file_bytes.end | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | store.file_bytes.max | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | store.file_bytes.start | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | store.freelist_page_count.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | store.freelist_page_count.end | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | store.freelist_page_count.max | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | store.freelist_page_count.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | store.live_page_bytes.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | store.live_page_bytes.end | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | store.live_page_bytes.max | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | store.live_page_bytes.start | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | store.page_count.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | store.page_count.end | 3 | 9554 | 9554 | 9554 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | store.page_count.max | 3 | 9554 | 9554 | 9554 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | store.page_count.start | 3 | 9554 | 9554 | 9554 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | verified.canonical-verification.canonical_Chunk_bytes | 3 | 524854978 | 524854978 | 524854978 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | verified.canonical-verification.canonical_Chunk_objects | 3 | 26998 | 26998 | 26998 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | verified.canonical-verification.canonical_DirectoryNode_bytes | 3 | 89 | 89 | 89 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | verified.canonical-verification.canonical_DirectoryNode_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | verified.canonical-verification.canonical_DirectoryState_bytes | 3 | 98 | 98 | 98 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | verified.canonical-verification.canonical_DirectoryState_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | verified.canonical-verification.canonical_FileNode_bytes | 3 | 1099692 | 1099692 | 1099692 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | verified.canonical-verification.canonical_FileNode_objects | 3 | 217 | 217 | 217 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | verified.canonical-verification.canonical_FileState_bytes | 3 | 424 | 424 | 424 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | verified.canonical-verification.canonical_FileState_objects | 3 | 4 | 4 | 4 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | verified.canonical-verification.canonical_InodeRecord_bytes | 3 | 196 | 196 | 196 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | verified.canonical-verification.canonical_InodeRecord_objects | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | verified.canonical-verification.canonical_InodeTable_bytes | 3 | 172 | 172 | 172 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | verified.canonical-verification.canonical_InodeTable_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | verified.canonical-verification.canonical_Metadata_bytes | 3 | 286 | 286 | 286 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | verified.canonical-verification.canonical_Metadata_objects | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | verified.canonical-verification.canonical_Namespace_bytes | 3 | 121 | 121 | 121 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | verified.canonical-verification.canonical_Namespace_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | verified.canonical-verification.canonical_unique_bytes | 3 | 525956056 | 525956056 | 525956056 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | verified.canonical-verification.canonical_unique_objects | 3 | 27227 | 27227 | 27227 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | verified.canonical-verification.independent_content_paths | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | verified.canonical-verification.logical_bytes | 3 | 524288000 | 524288000 | 524288000 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | verified.canonical-verification.persistence_custody_paths | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | verified.canonical-verification.verified_paths | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | verified.canonical-verification.verified_regular_paths | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | visibility_ns | 3 | 127792 | 103000 | 152792 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | visited_file_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | visited_path_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | workload_chmod_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | workload_close_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | workload_closedir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | workload_fsync_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | workload_fsyncdir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | workload_ftruncate_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | workload_lstat_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | workload_mkdir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | workload_ns | 3 | 9027417 | 8214333 | 9497042 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | workload_open_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | workload_open_directory_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | workload_opendir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | workload_plan_ns | 3 | 4084 | 3042 | 20667 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | workload_pread_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | workload_pwrite_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | workload_rename_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | workload_rmdir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | workload_symlink_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-1 | workload_unlink_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | attempted_syscall_count | 3 | 12 | 12 | 12 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | benchmark_injection_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | benchmark_reopen_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | benchmark_verifier_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | cache_acquisition_ns | 3 | 10768334 | 10222833 | 19511250 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | cache_build_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | cache_validation_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | cgroup.cpu_usage_usec_delta | 3 | 23825 | 22884 | 25798 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | cgroup.cpu_usage_usec_end | 3 | 68628 | 66318 | 70108 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | cgroup.cpu_usage_usec_start | 3 | 44310 | 43434 | 44803 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | cgroup.observed_max.memory.current | 3 | 6995968 | 6803456 | 7196672 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | cgroup.observed_max.memory.events.oom | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | cgroup.observed_max.memory.events.oom_kill | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | cgroup.observed_max.memory.peak | 3 | 8216576 | 7925760 | 8466432 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | cgroup.observed_max.memory.stat.anon | 3 | 5271552 | 5263360 | 5271552 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | cgroup.observed_max.memory.stat.file | 3 | 86016 | 86016 | 86016 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | cgroup.observed_max.memory.stat.file_dirty | 3 | 4096 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | cgroup.observed_max.memory.stat.file_writeback | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | cgroup.observed_max.memory.stat.kernel | 3 | 1318912 | 1314816 | 1323008 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | cgroup.observed_max.memory.stat.shmem | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | cgroup.observed_max.memory.stat.slab | 3 | 678952 | 678552 | 679824 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | cgroup.observed_max.memory.swap.current | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | cgroup.observed_max.pids.current | 3 | 17 | 17 | 17 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | cleanup_ns | 3 | 301435542 | 291966791 | 324196166 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | clone_bytes | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | clone_wall_ns | 3 | 410728709 | 404530083 | 414362875 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | command_wall_ns | 3 | 1316546042 | 1315733541 | 1360840500 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_diagnostics.cdc_bytes_scanned | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_diagnostics.edit_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_diagnostics.edit_metric_nodes_scanned | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_diagnostics.edit_piece_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_diagnostics.edit_piece_height | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_diagnostics.edit_piece_logical_charge | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_diagnostics.edit_spool_allocated_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_diagnostics.edit_spool_live_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_diagnostics.edit_spool_peak_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_diagnostics.edit_spool_superseded_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_diagnostics.edit_tree_visits | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_diagnostics.namespace_base_paths_visited | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_diagnostics.namespace_candidate_probe_nodes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_diagnostics.namespace_clean_nodes_visited | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_diagnostics.namespace_dirty_nodes_visited | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_diagnostics.namespace_final_paths_visited | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_diagnostics.physical_spool_allocated_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_diagnostics.physical_spool_observation_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_diagnostics.physical_spool_observation_errors | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_diagnostics.physical_spool_peak_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_ns | 3 | 1639292 | 1359166 | 1713500 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.candidate_finish_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.candidate_plan_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.capture_ns | 3 | 9083 | 8792 | 10625 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.captured_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.captured_files | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.content_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.dirty_compare_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.in_place_rebase_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.local_admission_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.max_admission_transaction_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.max_admission_transaction_objects | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.namespace_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.object_admission_begin_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.object_admission_commit_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.object_admission_insert_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.object_admission_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.object_admission_transactions | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.pause_fence_ns | 3 | 743417 | 595708 | 840208 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.payload_bytes_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.publication_begin_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.publication_commit_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.publication_insert_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.publication_metadata_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.publication_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.publication_payload_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.quiesce_ns | 3 | 375 | 375 | 459 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.resume_ns | 3 | 277917 | 249125 | 305833 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.snapshot_database_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.snapshot_database_calls | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.snapshot_database_rows | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.total_ns | 3 | 1635709 | 1356208 | 1709625 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | commit_work.unattributed_ns | 3 | 605208 | 445209 | 609208 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | completed_chain_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | completed_episode_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | completed_file_write_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | completed_read_bytes | 3 | 40960 | 40960 | 40960 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | completed_read_request_count | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | completed_syscall_count | 3 | 12 | 12 | 12 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | completed_target_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | completed_write_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | create_ns | 3 | 8877000 | 8273542 | 10195875 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | created_commit_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | directory_entry_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | editor_save_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | end_ns | 3 | 3276583 | 3259459 | 4626125 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | exec_ns | 3 | 63365083 | 62100334 | 66411000 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | external_process_wall_ns | 3 | 159477750 | 157901875 | 160393250 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.callback_access | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.callback_create | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.callback_flush | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.callback_fsync | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.callback_fsyncdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.callback_getattr | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.callback_link | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.callback_lookup | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.callback_mkdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.callback_mknod | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.callback_open | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.callback_opendir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.callback_read | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.callback_readdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.callback_readdirplus | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.callback_readlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.callback_release | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.callback_releasedir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.callback_rename | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.callback_rmdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.callback_setattr | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.callback_statfs | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.callback_symlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.callback_unlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.callback_write | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.client_decode_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.client_decode_ns | 3 | 1493042 | 1076169 | 1677082 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.client_response_bytes | 3 | 20971610 | 20971610 | 20971610 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.client_response_frames | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.client_socket_read_ns | 3 | 56899500 | 55131002 | 58577793 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.collection_ns | 3 | 629041 | 496917 | 1088667 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.directory_entries_returned | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.directory_nonzero_offset_requests | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.host_dispatch_ns | 3 | 32908211 | 32577499 | 33312583 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.host_encode_ns | 3 | 209 | 207 | 209 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.host_response_bytes | 3 | 20971610 | 20971610 | 20971610 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.host_response_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.host_response_frames | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.host_socket_write_ns | 3 | 4513707 | 4346252 | 5142958 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.init_capabilities | 3 | 4481057 | 4481057 | 4481057 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.kernel_read_bytes | 3 | 81920 | 81920 | 81920 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.kernel_read_gt_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.kernel_read_le_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.kernel_read_le_256k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.kernel_read_le_4k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.kernel_read_le_64k | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.kernel_read_requests | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.local_bytes | 3 | 21338618 | 21331202 | 21345836 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.local_calls | 3 | 71 | 68 | 73 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.local_ids | 3 | 1148 | 1146 | 1151 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.local_read_auth_ns | 3 | 31823701 | 31712336 | 32289496 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.local_rows | 3 | 1148 | 1146 | 1151 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.max_payload_batch | 3 | 113 | 111 | 114 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.max_readahead_bytes | 3 | 2097152 | 2097152 | 2097152 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.payload_batches | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.payload_bytes_read | 3 | 20971520 | 20971520 | 20971520 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.payload_ids | 3 | 1088 | 1087 | 1088 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.read_ahead_cache_copy_bytes | 3 | 81920 | 81920 | 81920 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.read_ahead_fetched_bytes | 3 | 20971520 | 20971520 | 20971520 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.read_ahead_fetches | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.read_ahead_hits | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.read_ahead_misses | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.read_ahead_requested_bytes | 3 | 20971520 | 20971520 | 20971520 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.read_ahead_served_bytes | 3 | 81920 | 81920 | 81920 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.read_ahead_unused_bytes | 3 | 20889600 | 20889600 | 20889600 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.read_plan_builds | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.rope_nodes_read | 3 | 48 | 45 | 50 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.snapshot_cache_bytes | 3 | 2858 | 2858 | 2858 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.snapshot_cache_hits | 3 | 24 | 24 | 24 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.snapshot_cache_rows | 3 | 24 | 24 | 24 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.snapshot_database_bytes | 3 | 21335760 | 21328344 | 21342978 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.snapshot_database_calls | 3 | 47 | 44 | 49 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.snapshot_database_rows | 3 | 1124 | 1122 | 1127 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.workspace_output_bytes | 3 | 20971520 | 20971520 | 20971520 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.workspace_read_calls | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.workspace_read_ns | 3 | 32870417 | 32547459 | 33256668 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_read.workspace_requested_bytes | 3 | 20971520 | 20971520 | 20971520 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_write.client_frame_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_write.client_request_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_write.collection_ns | 3 | 523917 | 385084 | 542083 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_write.decode_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_write.encode_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_write.frame_payload_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_write.host_decode_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_write.host_dispatch_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_write.host_frame_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_write.kernel_write_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_write.kernel_write_gt_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_write.kernel_write_le_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_write.kernel_write_le_256k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_write.kernel_write_le_4k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_write.kernel_write_le_64k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_write.kernel_write_requests | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_write.max_write_bytes | 3 | 1048576 | 1048576 | 1048576 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_write.socket_read_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_write.socket_write_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_write.spool_write_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_write.spool_write_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_write.spool_write_open_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_write.workspace_fence_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | fuse_write.workspace_fence_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | git_process_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | host.disk_read_bytes.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | host.disk_read_bytes.end | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | host.disk_read_bytes.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | host.disk_write_bytes.delta | 3 | 12288 | 8192 | 12288 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | host.disk_write_bytes.end | 3 | 12288 | 8192 | 12288 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | host.disk_write_bytes.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | host.peak_resident_bytes.max | 3 | 50659328 | 49905664 | 52445184 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | host.physical_footprint_bytes.max | 3 | 13664784 | 11698680 | 13926904 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | host.resident_bytes.max | 3 | 50626560 | 49872896 | 52445184 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | host.swaps.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | host.swaps.end | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | host.swaps.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | host.system_cpu_ns.delta | 3 | 38256250 | 37950959 | 39467625 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | host.system_cpu_ns.end | 3 | 40075041 | 39966875 | 41306125 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | host.system_cpu_ns.start | 3 | 1838500 | 1818791 | 2015916 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | host.user_cpu_ns.delta | 3 | 37150125 | 36590333 | 37150291 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | host.user_cpu_ns.end | 3 | 38997541 | 38411958 | 39026041 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | host.user_cpu_ns.start | 3 | 1847250 | 1821625 | 1875916 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | host_orchestration_ns | 3 | 107006458 | 105671667 | 112039333 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | host_sampler.baseline_bytes | 3 | 2605056 | 2572288 | 2621440 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | host_sampler.final_bytes | 3 | 16515072 | 14581760 | 16809984 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | host_sampler.maximum_gap_ns | 3 | 12531625 | 12078583 | 12538041 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | host_sampler.sample_count | 3 | 13 | 13 | 13 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | host_sampler.sampled_peak_bytes | 3 | 50642944 | 49872896 | 52412416 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | inplace_edit_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | input.fixture_bytes | 3 | 524288000 | 524288000 | 524288000 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | input.regular_files | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | interrupted_syscall_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | lifecycle.anchor_prefetch_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | lifecycle.cleanup_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | lifecycle.docker_calls | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | lifecycle.docker_setup_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | lifecycle.helper_copy_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | lifecycle.mount_ready_ns | 3 | 7458500 | 6962209 | 8922292 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | lifecycle.proxy_ns | 3 | 177625 | 174458 | 183084 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | lifecycle.small_file_prefetch_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | lifecycle.small_file_prefetch_eligible | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | lifecycle.snapshot_cache_bytes_at_create | 3 | 1612 | 1612 | 1612 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | lifecycle.snapshot_cache_rows_at_create | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | lifecycle.snapshot_database_bytes | 3 | 972 | 972 | 972 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | lifecycle.snapshot_database_calls | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | lifecycle.snapshot_database_rows | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | lifecycle.snapshot_store_wide_scans | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | lifecycle.total_ns | 3 | 9889001 | 9588167 | 11082834 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | lifecycle.unattributed_ns | 3 | 258917 | 249500 | 260624 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | lifecycle.unmount_ns | 3 | 799042 | 686709 | 831792 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | lifecycle.wait_ns | 3 | 999250 | 934375 | 1661625 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | metadata_normalization_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | orchestration_unattributed_ns | 3 | 29668375 | 29287959 | 30354665 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | preparation_ns | 3 | 862505333 | 856351125 | 876298125 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | pure_call_sum_ns | 3 | 77718499 | 76003292 | 81684668 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | runtime_preparation_ns | 3 | 359358291 | 358103625 | 364660291 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | spool_boundary.max_allocated_bytes | 3 | 4096 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | spool_boundary.max_file_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | spool_boundary.max_logical_bytes | 3 | 150 | 150 | 150 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | store.allocated_bytes.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | store.allocated_bytes.end | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | store.allocated_bytes.max | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | store.allocated_bytes.start | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | store.file_bytes.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | store.file_bytes.end | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | store.file_bytes.max | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | store.file_bytes.start | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | store.freelist_page_count.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | store.freelist_page_count.end | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | store.freelist_page_count.max | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | store.freelist_page_count.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | store.live_page_bytes.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | store.live_page_bytes.end | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | store.live_page_bytes.max | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | store.live_page_bytes.start | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | store.page_count.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | store.page_count.end | 3 | 9554 | 9554 | 9554 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | store.page_count.max | 3 | 9554 | 9554 | 9554 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | store.page_count.start | 3 | 9554 | 9554 | 9554 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | verified.canonical-verification.canonical_Chunk_bytes | 3 | 524854978 | 524854978 | 524854978 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | verified.canonical-verification.canonical_Chunk_objects | 3 | 26998 | 26998 | 26998 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | verified.canonical-verification.canonical_DirectoryNode_bytes | 3 | 89 | 89 | 89 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | verified.canonical-verification.canonical_DirectoryNode_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | verified.canonical-verification.canonical_DirectoryState_bytes | 3 | 98 | 98 | 98 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | verified.canonical-verification.canonical_DirectoryState_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | verified.canonical-verification.canonical_FileNode_bytes | 3 | 1099692 | 1099692 | 1099692 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | verified.canonical-verification.canonical_FileNode_objects | 3 | 217 | 217 | 217 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | verified.canonical-verification.canonical_FileState_bytes | 3 | 424 | 424 | 424 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | verified.canonical-verification.canonical_FileState_objects | 3 | 4 | 4 | 4 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | verified.canonical-verification.canonical_InodeRecord_bytes | 3 | 196 | 196 | 196 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | verified.canonical-verification.canonical_InodeRecord_objects | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | verified.canonical-verification.canonical_InodeTable_bytes | 3 | 172 | 172 | 172 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | verified.canonical-verification.canonical_InodeTable_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | verified.canonical-verification.canonical_Metadata_bytes | 3 | 286 | 286 | 286 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | verified.canonical-verification.canonical_Metadata_objects | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | verified.canonical-verification.canonical_Namespace_bytes | 3 | 121 | 121 | 121 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | verified.canonical-verification.canonical_Namespace_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | verified.canonical-verification.canonical_unique_bytes | 3 | 525956056 | 525956056 | 525956056 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | verified.canonical-verification.canonical_unique_objects | 3 | 27227 | 27227 | 27227 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | verified.canonical-verification.independent_content_paths | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | verified.canonical-verification.logical_bytes | 3 | 524288000 | 524288000 | 524288000 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | verified.canonical-verification.persistence_custody_paths | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | verified.canonical-verification.verified_paths | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | verified.canonical-verification.verified_regular_paths | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | visibility_ns | 3 | 104834 | 94583 | 110083 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | visited_file_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | visited_path_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | workload_chmod_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | workload_close_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | workload_closedir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | workload_fsync_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | workload_fsyncdir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | workload_ftruncate_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | workload_lstat_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | workload_mkdir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | workload_ns | 3 | 60241875 | 58818959 | 61647708 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | workload_open_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | workload_open_directory_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | workload_opendir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | workload_plan_ns | 3 | 7542 | 6333 | 11000 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | workload_pread_call_count | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | workload_pwrite_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | workload_rename_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | workload_rmdir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | workload_symlink_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-10 | workload_unlink_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | attempted_syscall_count | 3 | 102 | 102 | 102 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | benchmark_injection_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | benchmark_reopen_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | benchmark_verifier_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | cache_acquisition_ns | 3 | 10978500 | 10682708 | 11279417 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | cache_build_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | cache_validation_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | cgroup.cpu_usage_usec_delta | 3 | 146104 | 144574 | 154026 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | cgroup.cpu_usage_usec_end | 3 | 190999 | 189294 | 199413 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | cgroup.cpu_usage_usec_start | 3 | 45387 | 43190 | 46425 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | cgroup.observed_max.memory.current | 3 | 8597504 | 8540160 | 8802304 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | cgroup.observed_max.memory.events.oom | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | cgroup.observed_max.memory.events.oom_kill | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | cgroup.observed_max.memory.peak | 3 | 9347072 | 9150464 | 9609216 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | cgroup.observed_max.memory.stat.anon | 3 | 5271552 | 5271552 | 5271552 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | cgroup.observed_max.memory.stat.file | 3 | 823296 | 823296 | 823296 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | cgroup.observed_max.memory.stat.file_dirty | 3 | 4096 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | cgroup.observed_max.memory.stat.file_writeback | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | cgroup.observed_max.memory.stat.kernel | 3 | 1384448 | 1376256 | 1388544 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | cgroup.observed_max.memory.stat.shmem | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | cgroup.observed_max.memory.stat.slab | 3 | 746128 | 745904 | 746216 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | cgroup.observed_max.memory.swap.current | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | cgroup.observed_max.pids.current | 3 | 17 | 17 | 17 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | cleanup_ns | 3 | 317603583 | 272273167 | 320386209 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | clone_bytes | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | clone_wall_ns | 3 | 438366750 | 425172084 | 511715167 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | command_wall_ns | 3 | 1927710042 | 1845687500 | 1976204375 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_diagnostics.cdc_bytes_scanned | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_diagnostics.edit_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_diagnostics.edit_metric_nodes_scanned | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_diagnostics.edit_piece_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_diagnostics.edit_piece_height | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_diagnostics.edit_piece_logical_charge | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_diagnostics.edit_spool_allocated_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_diagnostics.edit_spool_live_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_diagnostics.edit_spool_peak_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_diagnostics.edit_spool_superseded_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_diagnostics.edit_tree_visits | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_diagnostics.namespace_base_paths_visited | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_diagnostics.namespace_candidate_probe_nodes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_diagnostics.namespace_clean_nodes_visited | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_diagnostics.namespace_dirty_nodes_visited | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_diagnostics.namespace_final_paths_visited | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_diagnostics.physical_spool_allocated_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_diagnostics.physical_spool_observation_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_diagnostics.physical_spool_observation_errors | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_diagnostics.physical_spool_peak_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_ns | 3 | 1503875 | 1338541 | 1529500 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.candidate_finish_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.candidate_plan_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.capture_ns | 3 | 9917 | 9333 | 11083 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.captured_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.captured_files | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.content_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.dirty_compare_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.in_place_rebase_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.local_admission_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.max_admission_transaction_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.max_admission_transaction_objects | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.namespace_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.object_admission_begin_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.object_admission_commit_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.object_admission_insert_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.object_admission_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.object_admission_transactions | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.pause_fence_ns | 3 | 693167 | 691417 | 723667 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.payload_bytes_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.publication_begin_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.publication_commit_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.publication_insert_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.publication_metadata_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.publication_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.publication_payload_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.quiesce_ns | 3 | 375 | 208 | 459 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.resume_ns | 3 | 239042 | 219000 | 253917 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.snapshot_database_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.snapshot_database_calls | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.snapshot_database_rows | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.total_ns | 3 | 1497333 | 1335458 | 1526250 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | commit_work.unattributed_ns | 3 | 538958 | 414665 | 553833 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | completed_chain_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | completed_episode_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | completed_file_write_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | completed_read_bytes | 3 | 409600 | 409600 | 409600 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | completed_read_request_count | 3 | 100 | 100 | 100 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | completed_syscall_count | 3 | 102 | 102 | 102 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | completed_target_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | completed_write_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | create_ns | 3 | 10588125 | 9836417 | 10664916 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | created_commit_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | directory_entry_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | editor_save_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | end_ns | 3 | 3354666 | 2957041 | 3799583 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | exec_ns | 3 | 572435625 | 562814625 | 593787541 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | external_process_wall_ns | 3 | 677857458 | 674166208 | 678765291 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.callback_access | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.callback_create | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.callback_flush | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.callback_fsync | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.callback_fsyncdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.callback_getattr | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.callback_link | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.callback_lookup | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.callback_mkdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.callback_mknod | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.callback_open | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.callback_opendir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.callback_read | 3 | 100 | 100 | 100 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.callback_readdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.callback_readdirplus | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.callback_readlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.callback_release | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.callback_releasedir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.callback_rename | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.callback_rmdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.callback_setattr | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.callback_statfs | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.callback_symlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.callback_unlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.callback_write | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.client_decode_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.client_decode_ns | 3 | 11991541 | 11106371 | 12439669 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.client_response_bytes | 3 | 209716100 | 208184196 | 209716100 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.client_response_frames | 3 | 100 | 100 | 100 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.client_socket_read_ns | 3 | 539895699 | 530196457 | 562577461 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.collection_ns | 3 | 469833 | 455375 | 477750 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.directory_entries_returned | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.directory_nonzero_offset_requests | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.host_dispatch_ns | 3 | 302126875 | 301528926 | 305811961 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.host_encode_ns | 3 | 2296 | 2161 | 2458 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.host_response_bytes | 3 | 209716100 | 208184196 | 209716100 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.host_response_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.host_response_frames | 3 | 100 | 100 | 100 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.host_socket_write_ns | 3 | 42764413 | 42643785 | 42771378 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.init_capabilities | 3 | 4481057 | 4481057 | 4481057 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.kernel_read_bytes | 3 | 819200 | 819200 | 819200 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.kernel_read_gt_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.kernel_read_le_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.kernel_read_le_256k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.kernel_read_le_4k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.kernel_read_le_64k | 3 | 100 | 100 | 100 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.kernel_read_requests | 3 | 100 | 100 | 100 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.local_bytes | 3 | 213540840 | 211868279 | 213671081 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.local_calls | 3 | 595 | 592 | 598 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.local_ids | 3 | 11351 | 11320 | 11393 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.local_read_auth_ns | 3 | 292274666 | 292247904 | 295913250 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.local_rows | 3 | 11351 | 11320 | 11393 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.max_payload_batch | 3 | 115 | 114 | 119 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.max_readahead_bytes | 3 | 2097152 | 2097152 | 2097152 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.payload_batches | 3 | 100 | 100 | 100 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.payload_bytes_read | 3 | 209715200 | 208183296 | 209715200 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.payload_ids | 3 | 10853 | 10828 | 10898 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.read_ahead_cache_copy_bytes | 3 | 819200 | 819200 | 819200 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.read_ahead_fetched_bytes | 3 | 209715200 | 208183296 | 209715200 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.read_ahead_fetches | 3 | 100 | 100 | 100 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.read_ahead_hits | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.read_ahead_misses | 3 | 100 | 100 | 100 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.read_ahead_requested_bytes | 3 | 209715200 | 209715200 | 209715200 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.read_ahead_served_bytes | 3 | 819200 | 819200 | 819200 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.read_ahead_unused_bytes | 3 | 208896000 | 207364096 | 208896000 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.read_plan_builds | 3 | 100 | 100 | 100 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.rope_nodes_read | 3 | 482 | 479 | 485 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.snapshot_cache_bytes | 3 | 24998 | 24998 | 24998 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.snapshot_cache_hits | 3 | 204 | 204 | 204 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.snapshot_cache_rows | 3 | 204 | 204 | 204 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.snapshot_database_bytes | 3 | 213515842 | 211843281 | 213646083 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.snapshot_database_calls | 3 | 391 | 388 | 394 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.snapshot_database_rows | 3 | 11147 | 11116 | 11189 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.workspace_output_bytes | 3 | 209715200 | 208183296 | 209715200 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.workspace_read_calls | 3 | 100 | 100 | 100 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.workspace_read_ns | 3 | 301773080 | 301252251 | 305505087 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_read.workspace_requested_bytes | 3 | 209715200 | 208183296 | 209715200 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_write.client_frame_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_write.client_request_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_write.collection_ns | 3 | 425667 | 342292 | 479292 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_write.decode_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_write.encode_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_write.frame_payload_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_write.host_decode_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_write.host_dispatch_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_write.host_frame_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_write.kernel_write_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_write.kernel_write_gt_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_write.kernel_write_le_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_write.kernel_write_le_256k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_write.kernel_write_le_4k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_write.kernel_write_le_64k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_write.kernel_write_requests | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_write.max_write_bytes | 3 | 1048576 | 1048576 | 1048576 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_write.socket_read_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_write.socket_write_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_write.spool_write_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_write.spool_write_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_write.spool_write_open_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_write.workspace_fence_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | fuse_write.workspace_fence_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | git_process_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | host.disk_read_bytes.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | host.disk_read_bytes.end | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | host.disk_read_bytes.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | host.disk_write_bytes.delta | 3 | 8192 | 8192 | 12288 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | host.disk_write_bytes.end | 3 | 8192 | 8192 | 12288 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | host.disk_write_bytes.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | host.peak_resident_bytes.max | 3 | 59342848 | 59244544 | 59408384 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | host.physical_footprint_bytes.max | 3 | 14647800 | 14582288 | 14729744 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | host.resident_bytes.max | 3 | 59310080 | 59228160 | 59375616 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | host.swaps.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | host.swaps.end | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | host.swaps.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | host.system_cpu_ns.delta | 3 | 118806959 | 118357834 | 122480167 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | host.system_cpu_ns.end | 3 | 120509375 | 120014125 | 124185333 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | host.system_cpu_ns.start | 3 | 1702416 | 1656291 | 1705166 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | host.user_cpu_ns.delta | 3 | 268810167 | 266832291 | 268813625 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | host.user_cpu_ns.end | 3 | 270653333 | 268677416 | 270679333 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | host.user_cpu_ns.start | 3 | 1845125 | 1843166 | 1865708 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | host_orchestration_ns | 3 | 619710333 | 608547042 | 638812375 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | host_sampler.baseline_bytes | 3 | 2572288 | 2572288 | 2588672 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | host_sampler.final_bytes | 3 | 17514496 | 17432576 | 17596416 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | host_sampler.maximum_gap_ns | 3 | 12528208 | 12527542 | 12531250 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | host_sampler.sample_count | 3 | 61 | 58 | 62 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | host_sampler.sampled_peak_bytes | 3 | 59310080 | 59211776 | 59375616 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | inplace_edit_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | input.fixture_bytes | 3 | 524288000 | 524288000 | 524288000 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | input.regular_files | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | interrupted_syscall_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | lifecycle.anchor_prefetch_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | lifecycle.cleanup_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | lifecycle.docker_calls | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | lifecycle.docker_setup_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | lifecycle.helper_copy_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | lifecycle.mount_ready_ns | 3 | 9337750 | 8506500 | 9360500 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | lifecycle.proxy_ns | 3 | 171542 | 169417 | 181125 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | lifecycle.small_file_prefetch_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | lifecycle.small_file_prefetch_eligible | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | lifecycle.snapshot_cache_bytes_at_create | 3 | 1612 | 1612 | 1612 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | lifecycle.snapshot_cache_rows_at_create | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | lifecycle.snapshot_database_bytes | 3 | 972 | 972 | 972 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | lifecycle.snapshot_database_calls | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | lifecycle.snapshot_database_rows | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | lifecycle.snapshot_store_wide_scans | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | lifecycle.total_ns | 3 | 11258751 | 11244333 | 11636458 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | lifecycle.unattributed_ns | 3 | 271791 | 262833 | 277499 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | lifecycle.unmount_ns | 3 | 806792 | 749584 | 1543542 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | lifecycle.wait_ns | 3 | 737042 | 735667 | 1027958 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | metadata_normalization_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | orchestration_unattributed_ns | 3 | 30133792 | 29951752 | 32047418 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | preparation_ns | 3 | 928429042 | 893765375 | 983577542 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | pure_call_sum_ns | 3 | 587662915 | 578413250 | 608860623 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | runtime_preparation_ns | 3 | 377620167 | 376553208 | 395474416 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | spool_boundary.max_allocated_bytes | 3 | 4096 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | spool_boundary.max_file_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | spool_boundary.max_logical_bytes | 3 | 151 | 151 | 151 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | store.allocated_bytes.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | store.allocated_bytes.end | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | store.allocated_bytes.max | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | store.allocated_bytes.start | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | store.file_bytes.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | store.file_bytes.end | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | store.file_bytes.max | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | store.file_bytes.start | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | store.freelist_page_count.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | store.freelist_page_count.end | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | store.freelist_page_count.max | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | store.freelist_page_count.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | store.live_page_bytes.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | store.live_page_bytes.end | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | store.live_page_bytes.max | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | store.live_page_bytes.start | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | store.page_count.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | store.page_count.end | 3 | 9554 | 9554 | 9554 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | store.page_count.max | 3 | 9554 | 9554 | 9554 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | store.page_count.start | 3 | 9554 | 9554 | 9554 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | verified.canonical-verification.canonical_Chunk_bytes | 3 | 524854978 | 524854978 | 524854978 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | verified.canonical-verification.canonical_Chunk_objects | 3 | 26998 | 26998 | 26998 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | verified.canonical-verification.canonical_DirectoryNode_bytes | 3 | 89 | 89 | 89 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | verified.canonical-verification.canonical_DirectoryNode_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | verified.canonical-verification.canonical_DirectoryState_bytes | 3 | 98 | 98 | 98 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | verified.canonical-verification.canonical_DirectoryState_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | verified.canonical-verification.canonical_FileNode_bytes | 3 | 1099692 | 1099692 | 1099692 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | verified.canonical-verification.canonical_FileNode_objects | 3 | 217 | 217 | 217 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | verified.canonical-verification.canonical_FileState_bytes | 3 | 424 | 424 | 424 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | verified.canonical-verification.canonical_FileState_objects | 3 | 4 | 4 | 4 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | verified.canonical-verification.canonical_InodeRecord_bytes | 3 | 196 | 196 | 196 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | verified.canonical-verification.canonical_InodeRecord_objects | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | verified.canonical-verification.canonical_InodeTable_bytes | 3 | 172 | 172 | 172 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | verified.canonical-verification.canonical_InodeTable_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | verified.canonical-verification.canonical_Metadata_bytes | 3 | 286 | 286 | 286 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | verified.canonical-verification.canonical_Metadata_objects | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | verified.canonical-verification.canonical_Namespace_bytes | 3 | 121 | 121 | 121 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | verified.canonical-verification.canonical_Namespace_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | verified.canonical-verification.canonical_unique_bytes | 3 | 525956056 | 525956056 | 525956056 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | verified.canonical-verification.canonical_unique_objects | 3 | 27227 | 27227 | 27227 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | verified.canonical-verification.independent_content_paths | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | verified.canonical-verification.logical_bytes | 3 | 524288000 | 524288000 | 524288000 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | verified.canonical-verification.persistence_custody_paths | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | verified.canonical-verification.verified_paths | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | verified.canonical-verification.verified_regular_paths | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | visibility_ns | 3 | 101458 | 98541 | 126334 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | visited_file_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | visited_path_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | workload_chmod_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | workload_close_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | workload_closedir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | workload_fsync_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | workload_fsyncdir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | workload_ftruncate_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | workload_lstat_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | workload_mkdir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | workload_ns | 3 | 569199084 | 557860083 | 590004459 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | workload_open_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | workload_open_directory_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | workload_opendir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | workload_plan_ns | 3 | 39500 | 38875 | 43875 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | workload_pread_call_count | 3 | 100 | 100 | 100 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | workload_pwrite_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | workload_rename_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | workload_rmdir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | workload_symlink_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-100 | workload_unlink_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | attempted_syscall_count | 3 | 502 | 502 | 502 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | benchmark_injection_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | benchmark_reopen_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | benchmark_verifier_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | cache_acquisition_ns | 3 | 11657042 | 10680625 | 11763500 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | cache_build_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | cache_validation_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | cgroup.cpu_usage_usec_delta | 3 | 721187 | 670814 | 771811 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | cgroup.cpu_usage_usec_end | 3 | 765606 | 715358 | 817448 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | cgroup.cpu_usage_usec_start | 3 | 44544 | 44419 | 45637 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | cgroup.observed_max.memory.current | 3 | 11603968 | 11386880 | 12255232 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | cgroup.observed_max.memory.events.oom | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | cgroup.observed_max.memory.events.oom_kill | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | cgroup.observed_max.memory.peak | 3 | 12894208 | 12709888 | 13373440 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | cgroup.observed_max.memory.stat.anon | 3 | 5271552 | 5267456 | 5275648 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | cgroup.observed_max.memory.stat.file | 3 | 4091904 | 4075520 | 4096000 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | cgroup.observed_max.memory.stat.file_dirty | 3 | 4096 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | cgroup.observed_max.memory.stat.file_writeback | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | cgroup.observed_max.memory.stat.kernel | 3 | 1585152 | 1576960 | 1593344 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | cgroup.observed_max.memory.stat.shmem | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | cgroup.observed_max.memory.stat.slab | 3 | 949880 | 946144 | 954024 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | cgroup.observed_max.memory.swap.current | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | cgroup.observed_max.pids.current | 3 | 17 | 17 | 17 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | cleanup_ns | 3 | 303874833 | 280241167 | 399813917 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | clone_bytes | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | clone_wall_ns | 3 | 425241417 | 422575250 | 431884791 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | command_wall_ns | 3 | 4162351542 | 4064406042 | 4793606500 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_diagnostics.cdc_bytes_scanned | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_diagnostics.edit_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_diagnostics.edit_metric_nodes_scanned | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_diagnostics.edit_piece_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_diagnostics.edit_piece_height | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_diagnostics.edit_piece_logical_charge | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_diagnostics.edit_spool_allocated_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_diagnostics.edit_spool_live_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_diagnostics.edit_spool_peak_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_diagnostics.edit_spool_superseded_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_diagnostics.edit_tree_visits | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_diagnostics.namespace_base_paths_visited | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_diagnostics.namespace_candidate_probe_nodes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_diagnostics.namespace_clean_nodes_visited | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_diagnostics.namespace_dirty_nodes_visited | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_diagnostics.namespace_final_paths_visited | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_diagnostics.physical_spool_allocated_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_diagnostics.physical_spool_observation_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_diagnostics.physical_spool_observation_errors | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_diagnostics.physical_spool_peak_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_ns | 3 | 1435667 | 1425542 | 1723458 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.candidate_finish_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.candidate_plan_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.capture_ns | 3 | 16583 | 11292 | 17250 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.captured_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.captured_files | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.content_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.dirty_compare_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.in_place_rebase_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.local_admission_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.max_admission_transaction_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.max_admission_transaction_objects | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.namespace_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.object_admission_begin_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.object_admission_commit_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.object_admission_insert_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.object_admission_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.object_admission_transactions | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.pause_fence_ns | 3 | 662625 | 617500 | 824375 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.payload_bytes_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.publication_begin_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.publication_commit_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.publication_insert_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.publication_metadata_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.publication_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.publication_payload_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.quiesce_ns | 3 | 375 | 333 | 584 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.resume_ns | 3 | 228083 | 214792 | 266459 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.snapshot_database_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.snapshot_database_calls | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.snapshot_database_rows | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.total_ns | 3 | 1429500 | 1419459 | 1716958 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | commit_work.unattributed_ns | 3 | 562209 | 483500 | 659957 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | completed_chain_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | completed_episode_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | completed_file_write_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | completed_read_bytes | 3 | 2048000 | 2048000 | 2048000 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | completed_read_request_count | 3 | 500 | 500 | 500 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | completed_syscall_count | 3 | 502 | 502 | 502 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | completed_target_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | completed_write_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | create_ns | 3 | 10289125 | 9681667 | 11466666 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | created_commit_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | directory_entry_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | editor_save_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | end_ns | 3 | 4367042 | 3259333 | 4457792 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | exec_ns | 3 | 2775515583 | 2758525541 | 3528659709 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | external_process_wall_ns | 3 | 2879119958 | 2848754042 | 3625478834 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.callback_access | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.callback_create | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.callback_flush | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.callback_fsync | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.callback_fsyncdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.callback_getattr | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.callback_link | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.callback_lookup | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.callback_mkdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.callback_mknod | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.callback_open | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.callback_opendir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.callback_read | 3 | 498 | 498 | 499 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.callback_readdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.callback_readdirplus | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.callback_readlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.callback_release | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.callback_releasedir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.callback_rename | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.callback_rmdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.callback_setattr | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.callback_statfs | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.callback_symlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.callback_unlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.callback_write | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.client_decode_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.client_decode_ns | 3 | 61327086 | 47957213 | 74262042 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.client_response_bytes | 3 | 1040220546 | 1030582613 | 1042047353 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.client_response_frames | 3 | 497 | 493 | 498 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.client_socket_read_ns | 3 | 2626779149 | 2626736457 | 3399990598 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.collection_ns | 3 | 529375 | 490166 | 565250 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.directory_entries_returned | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.directory_nonzero_offset_requests | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.host_dispatch_ns | 3 | 1510580300 | 1492914637 | 1511945568 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.host_encode_ns | 3 | 11720 | 11453 | 12252 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.host_response_bytes | 3 | 1040220546 | 1030582613 | 1042047353 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.host_response_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.host_response_frames | 3 | 497 | 493 | 498 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.host_socket_write_ns | 3 | 222737333 | 208229933 | 223069467 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.init_capabilities | 3 | 4481057 | 4481057 | 4481057 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.kernel_read_bytes | 3 | 4087808 | 4071424 | 4091904 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.kernel_read_gt_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.kernel_read_le_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.kernel_read_le_256k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.kernel_read_le_4k | 3 | 0 | 0 | 2 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.kernel_read_le_64k | 3 | 498 | 496 | 499 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.kernel_read_requests | 3 | 498 | 498 | 499 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.local_bytes | 3 | 1059323010 | 1049240049 | 1060548500 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.local_calls | 3 | 2918 | 2892 | 2922 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.local_ids | 3 | 56501 | 55895 | 56600 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.local_read_auth_ns | 3 | 1459340023 | 1445403546 | 1459472924 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.local_rows | 3 | 56501 | 55895 | 56600 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.max_payload_batch | 3 | 115 | 114 | 119 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.max_readahead_bytes | 3 | 2097152 | 2097152 | 2097152 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.payload_batches | 3 | 497 | 493 | 498 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.payload_bytes_read | 3 | 1040216064 | 1030578176 | 1042042880 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.payload_ids | 3 | 54077 | 53496 | 54179 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.read_ahead_cache_copy_bytes | 3 | 4087808 | 4071424 | 4091904 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.read_ahead_fetched_bytes | 3 | 1040216064 | 1030578176 | 1042042880 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.read_ahead_fetches | 3 | 497 | 493 | 498 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.read_ahead_hits | 3 | 2 | 0 | 5 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.read_ahead_misses | 3 | 497 | 493 | 498 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.read_ahead_requested_bytes | 3 | 1042284544 | 1033895936 | 1044381696 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.read_ahead_served_bytes | 3 | 4087808 | 4071424 | 4091904 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.read_ahead_unused_bytes | 3 | 1036144640 | 1026490368 | 1037950976 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.read_plan_builds | 3 | 497 | 493 | 498 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.rope_nodes_read | 3 | 2408 | 2386 | 2411 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.snapshot_cache_bytes | 3 | 122660 | 121676 | 122906 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.snapshot_cache_hits | 3 | 998 | 990 | 1000 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.snapshot_cache_rows | 3 | 998 | 990 | 1000 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.snapshot_database_bytes | 3 | 1059200104 | 1049118373 | 1060425840 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.snapshot_database_calls | 3 | 1920 | 1902 | 1922 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.snapshot_database_rows | 3 | 55501 | 54905 | 55602 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.workspace_output_bytes | 3 | 1040216064 | 1030578176 | 1042042880 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.workspace_read_calls | 3 | 497 | 493 | 498 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.workspace_read_ns | 3 | 1508866656 | 1491424381 | 1510378535 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_read.workspace_requested_bytes | 3 | 1040216064 | 1030578176 | 1042042880 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_write.client_frame_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_write.client_request_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_write.collection_ns | 3 | 480792 | 401792 | 538083 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_write.decode_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_write.encode_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_write.frame_payload_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_write.host_decode_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_write.host_dispatch_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_write.host_frame_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_write.kernel_write_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_write.kernel_write_gt_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_write.kernel_write_le_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_write.kernel_write_le_256k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_write.kernel_write_le_4k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_write.kernel_write_le_64k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_write.kernel_write_requests | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_write.max_write_bytes | 3 | 1048576 | 1048576 | 1048576 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_write.socket_read_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_write.socket_write_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_write.spool_write_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_write.spool_write_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_write.spool_write_open_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_write.workspace_fence_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | fuse_write.workspace_fence_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | git_process_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | host.disk_read_bytes.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | host.disk_read_bytes.end | 3 | 0 | 0 | 65536 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | host.disk_read_bytes.start | 3 | 0 | 0 | 65536 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | host.disk_write_bytes.delta | 3 | 8192 | 8192 | 12288 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | host.disk_write_bytes.end | 3 | 8192 | 8192 | 12288 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | host.disk_write_bytes.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | host.peak_resident_bytes.max | 3 | 59768832 | 59506688 | 59817984 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | host.physical_footprint_bytes.max | 3 | 15090168 | 14828024 | 15172112 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | host.resident_bytes.max | 3 | 59752448 | 59490304 | 59817984 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | host.swaps.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | host.swaps.end | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | host.swaps.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | host.system_cpu_ns.delta | 3 | 493334459 | 474988583 | 496643375 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | host.system_cpu_ns.end | 3 | 496081750 | 476650166 | 499751041 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | host.system_cpu_ns.start | 3 | 2747291 | 1661583 | 3107666 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | host.user_cpu_ns.delta | 3 | 1286331458 | 1281623000 | 1296710625 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | host.user_cpu_ns.end | 3 | 1288203041 | 1283489666 | 1298596166 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | host.user_cpu_ns.start | 3 | 1871583 | 1866666 | 1885541 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | host_orchestration_ns | 3 | 2821421750 | 2806159916 | 3574002667 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | host_sampler.baseline_bytes | 3 | 2605056 | 2572288 | 2605056 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | host_sampler.final_bytes | 3 | 17956864 | 17694720 | 18022400 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | host_sampler.maximum_gap_ns | 3 | 12548750 | 12541916 | 13242625 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | host_sampler.sample_count | 3 | 260 | 258 | 325 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | host_sampler.sampled_peak_bytes | 3 | 59736064 | 59473920 | 59768832 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | inplace_edit_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | input.fixture_bytes | 3 | 524288000 | 524288000 | 524288000 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | input.regular_files | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | interrupted_syscall_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | lifecycle.anchor_prefetch_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | lifecycle.cleanup_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | lifecycle.docker_calls | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | lifecycle.docker_setup_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | lifecycle.helper_copy_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | lifecycle.mount_ready_ns | 3 | 8799000 | 8442833 | 10284792 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | lifecycle.proxy_ns | 3 | 188833 | 161042 | 205416 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | lifecycle.small_file_prefetch_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | lifecycle.small_file_prefetch_eligible | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | lifecycle.snapshot_cache_bytes_at_create | 3 | 1612 | 1612 | 1612 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | lifecycle.snapshot_cache_rows_at_create | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | lifecycle.snapshot_database_bytes | 3 | 972 | 972 | 972 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | lifecycle.snapshot_database_calls | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | lifecycle.snapshot_database_rows | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | lifecycle.snapshot_store_wide_scans | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | lifecycle.total_ns | 3 | 11702874 | 11108375 | 13010334 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | lifecycle.unattributed_ns | 3 | 283875 | 223125 | 328792 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | lifecycle.unmount_ns | 3 | 1021500 | 940958 | 1363292 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | lifecycle.wait_ns | 3 | 978083 | 753667 | 1846375 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | metadata_normalization_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | orchestration_unattributed_ns | 3 | 30244333 | 29958834 | 30249208 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | preparation_ns | 3 | 886971917 | 880496625 | 912832834 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | pure_call_sum_ns | 3 | 2791172542 | 2776201082 | 3543758334 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | runtime_preparation_ns | 3 | 371027791 | 351536583 | 393641292 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | spool_boundary.max_allocated_bytes | 3 | 4096 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | spool_boundary.max_file_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | spool_boundary.max_logical_bytes | 3 | 151 | 151 | 151 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | store.allocated_bytes.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | store.allocated_bytes.end | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | store.allocated_bytes.max | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | store.allocated_bytes.start | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | store.file_bytes.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | store.file_bytes.end | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | store.file_bytes.max | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | store.file_bytes.start | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | store.freelist_page_count.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | store.freelist_page_count.end | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | store.freelist_page_count.max | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | store.freelist_page_count.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | store.live_page_bytes.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | store.live_page_bytes.end | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | store.live_page_bytes.max | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | store.live_page_bytes.start | 3 | 626130944 | 626130944 | 626130944 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | store.page_count.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | store.page_count.end | 3 | 9554 | 9554 | 9554 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | store.page_count.max | 3 | 9554 | 9554 | 9554 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | store.page_count.start | 3 | 9554 | 9554 | 9554 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | verified.canonical-verification.canonical_Chunk_bytes | 3 | 524854978 | 524854978 | 524854978 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | verified.canonical-verification.canonical_Chunk_objects | 3 | 26998 | 26998 | 26998 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | verified.canonical-verification.canonical_DirectoryNode_bytes | 3 | 89 | 89 | 89 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | verified.canonical-verification.canonical_DirectoryNode_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | verified.canonical-verification.canonical_DirectoryState_bytes | 3 | 98 | 98 | 98 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | verified.canonical-verification.canonical_DirectoryState_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | verified.canonical-verification.canonical_FileNode_bytes | 3 | 1099692 | 1099692 | 1099692 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | verified.canonical-verification.canonical_FileNode_objects | 3 | 217 | 217 | 217 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | verified.canonical-verification.canonical_FileState_bytes | 3 | 424 | 424 | 424 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | verified.canonical-verification.canonical_FileState_objects | 3 | 4 | 4 | 4 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | verified.canonical-verification.canonical_InodeRecord_bytes | 3 | 196 | 196 | 196 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | verified.canonical-verification.canonical_InodeRecord_objects | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | verified.canonical-verification.canonical_InodeTable_bytes | 3 | 172 | 172 | 172 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | verified.canonical-verification.canonical_InodeTable_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | verified.canonical-verification.canonical_Metadata_bytes | 3 | 286 | 286 | 286 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | verified.canonical-verification.canonical_Metadata_objects | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | verified.canonical-verification.canonical_Namespace_bytes | 3 | 121 | 121 | 121 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | verified.canonical-verification.canonical_Namespace_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | verified.canonical-verification.canonical_unique_bytes | 3 | 525956056 | 525956056 | 525956056 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | verified.canonical-verification.canonical_unique_objects | 3 | 27227 | 27227 | 27227 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | verified.canonical-verification.independent_content_paths | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | verified.canonical-verification.logical_bytes | 3 | 524288000 | 524288000 | 524288000 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | verified.canonical-verification.persistence_custody_paths | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | verified.canonical-verification.verified_paths | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | verified.canonical-verification.verified_regular_paths | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | visibility_ns | 3 | 114500 | 91958 | 118375 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | visited_file_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | visited_path_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | workload_chmod_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | workload_close_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | workload_closedir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | workload_fsync_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | workload_fsyncdir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | workload_ftruncate_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | workload_lstat_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | workload_mkdir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | workload_ns | 3 | 2771496793 | 2754849335 | 3524141876 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | workload_open_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | workload_open_directory_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | workload_opendir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | workload_plan_ns | 3 | 187125 | 175750 | 202583 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | workload_pread_call_count | 3 | 500 | 500 | 500 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | workload_pwrite_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | workload_rename_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | workload_rmdir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | workload_symlink_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | payload-random-read-500 | workload_unlink_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | attempted_syscall_count | 3 | 5 | 5 | 5 |
| corrected / `39b70a103d99f422` | tiny-create-1 | benchmark_injection_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | benchmark_reopen_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | benchmark_verifier_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | cache_acquisition_ns | 3 | 487958875 | 483782792 | 522222459 |
| corrected / `39b70a103d99f422` | tiny-create-1 | cache_build_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | cache_validation_ns | 3 | 484362542 | 480235125 | 517587875 |
| corrected / `39b70a103d99f422` | tiny-create-1 | candidate.admission_transactions | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | candidate.batch_inserted_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | candidate.batch_inserted_objects | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | candidate.candidate_bytes | 3 | 22828 | 22636 | 25068 |
| corrected / `39b70a103d99f422` | tiny-create-1 | candidate.candidate_objects | 3 | 19 | 19 | 19 |
| corrected / `39b70a103d99f422` | tiny-create-1 | candidate.final_inserted_bytes | 3 | 22247 | 22055 | 24487 |
| corrected / `39b70a103d99f422` | tiny-create-1 | candidate.final_inserted_objects | 3 | 12 | 12 | 12 |
| corrected / `39b70a103d99f422` | tiny-create-1 | candidate.inserted_bytes | 3 | 22247 | 22055 | 24487 |
| corrected / `39b70a103d99f422` | tiny-create-1 | candidate.inserted_objects | 3 | 12 | 12 | 12 |
| corrected / `39b70a103d99f422` | tiny-create-1 | candidate.max_transaction_bytes | 3 | 22247 | 22055 | 24487 |
| corrected / `39b70a103d99f422` | tiny-create-1 | candidate.max_transaction_objects | 3 | 12 | 12 | 12 |
| corrected / `39b70a103d99f422` | tiny-create-1 | candidate.preexisting_reused_bytes | 3 | 581 | 581 | 581 |
| corrected / `39b70a103d99f422` | tiny-create-1 | candidate.preexisting_reused_objects | 3 | 7 | 7 | 7 |
| corrected / `39b70a103d99f422` | tiny-create-1 | candidate.reused_bytes | 3 | 581 | 581 | 581 |
| corrected / `39b70a103d99f422` | tiny-create-1 | candidate.reused_objects | 3 | 7 | 7 | 7 |
| corrected / `39b70a103d99f422` | tiny-create-1 | cgroup.cpu_usage_usec_delta | 3 | 10207 | 9517 | 10286 |
| corrected / `39b70a103d99f422` | tiny-create-1 | cgroup.cpu_usage_usec_end | 3 | 55586 | 53577 | 56884 |
| corrected / `39b70a103d99f422` | tiny-create-1 | cgroup.cpu_usage_usec_start | 3 | 46069 | 43291 | 46677 |
| corrected / `39b70a103d99f422` | tiny-create-1 | cgroup.observed_max.memory.current | 3 | 2478080 | 2326528 | 3215360 |
| corrected / `39b70a103d99f422` | tiny-create-1 | cgroup.observed_max.memory.events.oom | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | cgroup.observed_max.memory.events.oom_kill | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | cgroup.observed_max.memory.peak | 3 | 4468736 | 4259840 | 5296128 |
| corrected / `39b70a103d99f422` | tiny-create-1 | cgroup.observed_max.memory.stat.anon | 3 | 864256 | 864256 | 864256 |
| corrected / `39b70a103d99f422` | tiny-create-1 | cgroup.observed_max.memory.stat.file | 3 | 4096 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | tiny-create-1 | cgroup.observed_max.memory.stat.file_dirty | 3 | 4096 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | tiny-create-1 | cgroup.observed_max.memory.stat.file_writeback | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | cgroup.observed_max.memory.stat.kernel | 3 | 1294336 | 1286144 | 1298432 |
| corrected / `39b70a103d99f422` | tiny-create-1 | cgroup.observed_max.memory.stat.shmem | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | cgroup.observed_max.memory.stat.slab | 3 | 656600 | 655328 | 658000 |
| corrected / `39b70a103d99f422` | tiny-create-1 | cgroup.observed_max.memory.swap.current | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | cgroup.observed_max.pids.current | 3 | 13 | 12 | 14 |
| corrected / `39b70a103d99f422` | tiny-create-1 | cleanup_ns | 3 | 317570250 | 305543167 | 324157792 |
| corrected / `39b70a103d99f422` | tiny-create-1 | clone_bytes | 3 | 672202752 | 672137216 | 672595968 |
| corrected / `39b70a103d99f422` | tiny-create-1 | clone_wall_ns | 3 | 451675291 | 440608750 | 458221167 |
| corrected / `39b70a103d99f422` | tiny-create-1 | command_wall_ns | 3 | 5233201500 | 5062958125 | 5784112000 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_diagnostics.cdc_bytes_scanned | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_diagnostics.edit_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_diagnostics.edit_metric_nodes_scanned | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_diagnostics.edit_piece_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_diagnostics.edit_piece_height | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_diagnostics.edit_piece_logical_charge | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_diagnostics.edit_spool_allocated_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_diagnostics.edit_spool_live_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_diagnostics.edit_spool_peak_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_diagnostics.edit_spool_superseded_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_diagnostics.edit_tree_visits | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_diagnostics.namespace_base_paths_visited | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_diagnostics.namespace_candidate_probe_nodes | 3 | 7 | 7 | 8 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_diagnostics.namespace_clean_nodes_visited | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_diagnostics.namespace_dirty_nodes_visited | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_diagnostics.namespace_final_paths_visited | 3 | 7 | 7 | 7 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_diagnostics.physical_spool_allocated_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_diagnostics.physical_spool_observation_count | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_diagnostics.physical_spool_observation_errors | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_diagnostics.physical_spool_peak_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_ns | 3 | 57272833 | 23974500 | 118078041 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.candidate_finish_ns | 3 | 81125 | 79458 | 90333 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.candidate_plan_ns | 3 | 541 | 500 | 542 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.capture_ns | 3 | 6792 | 6334 | 9959 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.captured_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.captured_files | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.content_ns | 3 | 315209 | 313417 | 318791 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.dirty_compare_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.in_place_rebase_ns | 3 | 1070959 | 1023583 | 1093875 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.local_admission_ns | 3 | 359292 | 354625 | 767459 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.max_admission_transaction_bytes | 3 | 22247 | 22055 | 24487 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.max_admission_transaction_objects | 3 | 12 | 12 | 12 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.namespace_ns | 3 | 398958 | 380958 | 417250 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.object_admission_begin_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.object_admission_commit_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.object_admission_insert_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.object_admission_ns | 3 | 5041 | 2791 | 5375 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.object_admission_transactions | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.pause_fence_ns | 3 | 429875 | 315834 | 557875 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.payload_bytes_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.publication_begin_ns | 3 | 3750 | 3625 | 26167 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.publication_commit_ns | 3 | 52634459 | 19806167 | 113912083 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.publication_insert_ns | 3 | 409376 | 230709 | 494585 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.publication_metadata_ns | 3 | 105542 | 75458 | 123000 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.publication_ns | 3 | 53282208 | 20119458 | 114435417 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.publication_payload_ns | 3 | 334 | 166 | 375 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.quiesce_ns | 3 | 208 | 167 | 333 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.resume_ns | 3 | 508750 | 459709 | 598584 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.snapshot_database_bytes | 3 | 121497 | 120537 | 134169 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.snapshot_database_calls | 3 | 41 | 41 | 41 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.snapshot_database_rows | 3 | 41 | 41 | 41 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.total_ns | 3 | 57269000 | 23966541 | 118073709 |
| corrected / `39b70a103d99f422` | tiny-create-1 | commit_work.unattributed_ns | 3 | 505375 | 467997 | 524293 |
| corrected / `39b70a103d99f422` | tiny-create-1 | completed_chain_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | completed_episode_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | completed_file_write_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-1 | completed_read_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | completed_read_request_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | completed_syscall_count | 3 | 5 | 5 | 5 |
| corrected / `39b70a103d99f422` | tiny-create-1 | completed_target_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-1 | completed_write_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | create_ns | 3 | 9151333 | 9095750 | 9169791 |
| corrected / `39b70a103d99f422` | tiny-create-1 | created_commit_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-1 | directory_entry_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | editor_save_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | end_ns | 3 | 3300459 | 2980167 | 5339667 |
| corrected / `39b70a103d99f422` | tiny-create-1 | exec_ns | 3 | 11473084 | 8987041 | 12491083 |
| corrected / `39b70a103d99f422` | tiny-create-1 | external_process_wall_ns | 3 | 155178583 | 121460000 | 216315667 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.callback_access | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.callback_create | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.callback_flush | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.callback_fsync | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.callback_fsyncdir | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.callback_getattr | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.callback_link | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.callback_lookup | 3 | 4 | 4 | 4 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.callback_mkdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.callback_mknod | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.callback_open | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.callback_opendir | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.callback_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.callback_readdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.callback_readdirplus | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.callback_readlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.callback_release | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.callback_releasedir | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.callback_rename | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.callback_rmdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.callback_setattr | 3 | 4 | 4 | 4 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.callback_statfs | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.callback_symlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.callback_unlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.callback_write | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.client_decode_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.client_decode_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.client_response_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.client_response_frames | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.client_socket_read_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.collection_ns | 3 | 468167 | 401959 | 657250 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.directory_entries_returned | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.directory_nonzero_offset_requests | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.host_dispatch_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.host_encode_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.host_response_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.host_response_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.host_response_frames | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.host_socket_write_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.init_capabilities | 3 | 4481057 | 4481057 | 4481057 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.kernel_read_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.kernel_read_gt_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.kernel_read_le_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.kernel_read_le_256k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.kernel_read_le_4k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.kernel_read_le_64k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.kernel_read_requests | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.local_bytes | 3 | 155730 | 154450 | 171538 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.local_calls | 3 | 156 | 156 | 156 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.local_ids | 3 | 156 | 156 | 156 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.local_read_auth_ns | 3 | 1806160 | 1631791 | 2011495 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.local_rows | 3 | 156 | 156 | 156 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.max_payload_batch | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.max_readahead_bytes | 3 | 131072 | 131072 | 131072 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.payload_batches | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.payload_bytes_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.payload_ids | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.read_ahead_cache_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.read_ahead_fetched_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.read_ahead_fetches | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.read_ahead_hits | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.read_ahead_misses | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.read_ahead_requested_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.read_ahead_served_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.read_ahead_unused_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.read_plan_builds | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.rope_nodes_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.snapshot_cache_bytes | 3 | 10521 | 10521 | 10521 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.snapshot_cache_hits | 3 | 101 | 101 | 101 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.snapshot_cache_rows | 3 | 101 | 101 | 101 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.snapshot_database_bytes | 3 | 145209 | 143929 | 161017 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.snapshot_database_calls | 3 | 55 | 55 | 55 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.snapshot_database_rows | 3 | 55 | 55 | 55 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.workspace_output_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.workspace_read_calls | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.workspace_read_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_read.workspace_requested_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_write.client_frame_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_write.client_request_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_write.collection_ns | 3 | 398417 | 372083 | 425458 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_write.decode_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_write.encode_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_write.frame_payload_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_write.host_decode_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_write.host_dispatch_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_write.host_frame_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_write.kernel_write_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_write.kernel_write_gt_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_write.kernel_write_le_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_write.kernel_write_le_256k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_write.kernel_write_le_4k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_write.kernel_write_le_64k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_write.kernel_write_requests | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_write.max_write_bytes | 3 | 1048576 | 1048576 | 1048576 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_write.socket_read_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_write.socket_write_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_write.spool_write_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_write.spool_write_ns | 3 | 150875 | 148209 | 153834 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_write.spool_write_open_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_write.workspace_fence_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-1 | fuse_write.workspace_fence_ns | 3 | 28833 | 19917 | 45792 |
| corrected / `39b70a103d99f422` | tiny-create-1 | git_process_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | host.disk_read_bytes.delta | 3 | 1953792 | 688128 | 3878912 |
| corrected / `39b70a103d99f422` | tiny-create-1 | host.disk_read_bytes.end | 3 | 1953792 | 688128 | 3878912 |
| corrected / `39b70a103d99f422` | tiny-create-1 | host.disk_read_bytes.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | host.disk_write_bytes.delta | 3 | 8192 | 8192 | 12288 |
| corrected / `39b70a103d99f422` | tiny-create-1 | host.disk_write_bytes.end | 3 | 8192 | 8192 | 12288 |
| corrected / `39b70a103d99f422` | tiny-create-1 | host.disk_write_bytes.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | host.peak_resident_bytes.max | 3 | 13025280 | 12976128 | 13336576 |
| corrected / `39b70a103d99f422` | tiny-create-1 | host.physical_footprint_bytes.max | 3 | 4981192 | 4866504 | 5046704 |
| corrected / `39b70a103d99f422` | tiny-create-1 | host.resident_bytes.max | 3 | 12976128 | 12943360 | 13303808 |
| corrected / `39b70a103d99f422` | tiny-create-1 | host.swaps.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | host.swaps.end | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | host.swaps.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | host.system_cpu_ns.delta | 3 | 46959792 | 34667458 | 56223708 |
| corrected / `39b70a103d99f422` | tiny-create-1 | host.system_cpu_ns.end | 3 | 48710500 | 36381833 | 57794333 |
| corrected / `39b70a103d99f422` | tiny-create-1 | host.system_cpu_ns.start | 3 | 1714375 | 1570625 | 1750708 |
| corrected / `39b70a103d99f422` | tiny-create-1 | host.user_cpu_ns.delta | 3 | 12786959 | 12675458 | 13248500 |
| corrected / `39b70a103d99f422` | tiny-create-1 | host.user_cpu_ns.end | 3 | 14745000 | 14506666 | 15048541 |
| corrected / `39b70a103d99f422` | tiny-create-1 | host.user_cpu_ns.start | 3 | 1831208 | 1800041 | 1958041 |
| corrected / `39b70a103d99f422` | tiny-create-1 | host_orchestration_ns | 3 | 113193084 | 78756792 | 176116500 |
| corrected / `39b70a103d99f422` | tiny-create-1 | host_sampler.baseline_bytes | 3 | 2588672 | 2588672 | 2588672 |
| corrected / `39b70a103d99f422` | tiny-create-1 | host_sampler.final_bytes | 3 | 8142848 | 8028160 | 8224768 |
| corrected / `39b70a103d99f422` | tiny-create-1 | host_sampler.maximum_gap_ns | 3 | 12518334 | 12504500 | 12523375 |
| corrected / `39b70a103d99f422` | tiny-create-1 | host_sampler.sample_count | 3 | 14 | 11 | 19 |
| corrected / `39b70a103d99f422` | tiny-create-1 | host_sampler.sampled_peak_bytes | 3 | 12992512 | 12943360 | 13287424 |
| corrected / `39b70a103d99f422` | tiny-create-1 | inplace_edit_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | input.fixture_bytes | 3 | 524288000 | 524288000 | 524288000 |
| corrected / `39b70a103d99f422` | tiny-create-1 | input.regular_files | 3 | 100000 | 100000 | 100000 |
| corrected / `39b70a103d99f422` | tiny-create-1 | interrupted_syscall_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | lifecycle.anchor_prefetch_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | lifecycle.cleanup_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | lifecycle.docker_calls | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | lifecycle.docker_setup_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | lifecycle.helper_copy_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | lifecycle.mount_ready_ns | 3 | 7697666 | 7655542 | 7765708 |
| corrected / `39b70a103d99f422` | tiny-create-1 | lifecycle.proxy_ns | 3 | 180750 | 173458 | 189709 |
| corrected / `39b70a103d99f422` | tiny-create-1 | lifecycle.small_file_prefetch_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | lifecycle.small_file_prefetch_eligible | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | lifecycle.snapshot_cache_bytes_at_create | 3 | 1376 | 1376 | 1376 |
| corrected / `39b70a103d99f422` | tiny-create-1 | lifecycle.snapshot_cache_rows_at_create | 3 | 9 | 9 | 9 |
| corrected / `39b70a103d99f422` | tiny-create-1 | lifecycle.snapshot_database_bytes | 3 | 11684 | 10852 | 11876 |
| corrected / `39b70a103d99f422` | tiny-create-1 | lifecycle.snapshot_database_calls | 3 | 12 | 12 | 12 |
| corrected / `39b70a103d99f422` | tiny-create-1 | lifecycle.snapshot_database_rows | 3 | 12 | 12 | 12 |
| corrected / `39b70a103d99f422` | tiny-create-1 | lifecycle.snapshot_store_wide_scans | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | lifecycle.total_ns | 3 | 9554667 | 9269209 | 9807958 |
| corrected / `39b70a103d99f422` | tiny-create-1 | lifecycle.unattributed_ns | 3 | 236834 | 197250 | 237999 |
| corrected / `39b70a103d99f422` | tiny-create-1 | lifecycle.unmount_ns | 3 | 625250 | 449167 | 730000 |
| corrected / `39b70a103d99f422` | tiny-create-1 | lifecycle.wait_ns | 3 | 753417 | 735417 | 1003667 |
| corrected / `39b70a103d99f422` | tiny-create-1 | metadata_normalization_count | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | tiny-create-1 | metadata_normalization_ns | 3 | 1708375 | 1454542 | 1823292 |
| corrected / `39b70a103d99f422` | tiny-create-1 | orchestration_unattributed_ns | 3 | 32029250 | 29711084 | 34679252 |
| corrected / `39b70a103d99f422` | tiny-create-1 | preparation_ns | 3 | 4753067250 | 4527834250 | 5356078083 |
| corrected / `39b70a103d99f422` | tiny-create-1 | pure_call_sum_ns | 3 | 78513832 | 49045708 | 144087250 |
| corrected / `39b70a103d99f422` | tiny-create-1 | root_sync_ns | 3 | 418042 | 325625 | 964416 |
| corrected / `39b70a103d99f422` | tiny-create-1 | runtime_preparation_ns | 3 | 358895458 | 350913166 | 384718291 |
| corrected / `39b70a103d99f422` | tiny-create-1 | spool_boundary.max_allocated_bytes | 3 | 4096 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | tiny-create-1 | spool_boundary.max_file_count | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | tiny-create-1 | spool_boundary.max_logical_bytes | 3 | 141 | 141 | 141 |
| corrected / `39b70a103d99f422` | tiny-create-1 | store.allocated_bytes.delta | 3 | 0 | 0 | 16777216 |
| corrected / `39b70a103d99f422` | tiny-create-1 | store.allocated_bytes.end | 3 | 672595968 | 672137216 | 688979968 |
| corrected / `39b70a103d99f422` | tiny-create-1 | store.allocated_bytes.max | 3 | 672595968 | 672137216 | 688979968 |
| corrected / `39b70a103d99f422` | tiny-create-1 | store.allocated_bytes.start | 3 | 672202752 | 672137216 | 672595968 |
| corrected / `39b70a103d99f422` | tiny-create-1 | store.file_bytes.delta | 3 | 0 | 0 | 65536 |
| corrected / `39b70a103d99f422` | tiny-create-1 | store.file_bytes.end | 3 | 672268288 | 672137216 | 672595968 |
| corrected / `39b70a103d99f422` | tiny-create-1 | store.file_bytes.max | 3 | 672268288 | 672137216 | 672595968 |
| corrected / `39b70a103d99f422` | tiny-create-1 | store.file_bytes.start | 3 | 672202752 | 672137216 | 672595968 |
| corrected / `39b70a103d99f422` | tiny-create-1 | store.freelist_page_count.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | store.freelist_page_count.end | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | store.freelist_page_count.max | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | store.freelist_page_count.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | store.live_page_bytes.delta | 3 | 0 | 0 | 65536 |
| corrected / `39b70a103d99f422` | tiny-create-1 | store.live_page_bytes.end | 3 | 672268288 | 672137216 | 672595968 |
| corrected / `39b70a103d99f422` | tiny-create-1 | store.live_page_bytes.max | 3 | 672268288 | 672137216 | 672595968 |
| corrected / `39b70a103d99f422` | tiny-create-1 | store.live_page_bytes.start | 3 | 672202752 | 672137216 | 672595968 |
| corrected / `39b70a103d99f422` | tiny-create-1 | store.page_count.delta | 3 | 0 | 0 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-1 | store.page_count.end | 3 | 10258 | 10256 | 10263 |
| corrected / `39b70a103d99f422` | tiny-create-1 | store.page_count.max | 3 | 10258 | 10256 | 10263 |
| corrected / `39b70a103d99f422` | tiny-create-1 | store.page_count.start | 3 | 10257 | 10256 | 10263 |
| corrected / `39b70a103d99f422` | tiny-create-1 | verified.canonical-verification.canonical_Chunk_bytes | 3 | 526559947 | 526558960 | 526560157 |
| corrected / `39b70a103d99f422` | tiny-create-1 | verified.canonical-verification.canonical_Chunk_objects | 3 | 108187 | 108140 | 108197 |
| corrected / `39b70a103d99f422` | tiny-create-1 | verified.canonical-verification.canonical_DirectoryNode_bytes | 3 | 4446919 | 4446919 | 4446919 |
| corrected / `39b70a103d99f422` | tiny-create-1 | verified.canonical-verification.canonical_DirectoryNode_objects | 3 | 1015 | 1015 | 1015 |
| corrected / `39b70a103d99f422` | tiny-create-1 | verified.canonical-verification.canonical_DirectoryState_bytes | 3 | 62230 | 62230 | 62230 |
| corrected / `39b70a103d99f422` | tiny-create-1 | verified.canonical-verification.canonical_DirectoryState_objects | 3 | 635 | 635 | 635 |
| corrected / `39b70a103d99f422` | tiny-create-1 | verified.canonical-verification.canonical_FileNode_bytes | 3 | 8727656 | 8725776 | 8728056 |
| corrected / `39b70a103d99f422` | tiny-create-1 | verified.canonical-verification.canonical_FileNode_objects | 3 | 100004 | 100004 | 100004 |
| corrected / `39b70a103d99f422` | tiny-create-1 | verified.canonical-verification.canonical_FileState_bytes | 3 | 10600424 | 10600424 | 10600424 |
| corrected / `39b70a103d99f422` | tiny-create-1 | verified.canonical-verification.canonical_FileState_objects | 3 | 100004 | 100004 | 100004 |
| corrected / `39b70a103d99f422` | tiny-create-1 | verified.canonical-verification.canonical_InodeRecord_bytes | 3 | 9862328 | 9862328 | 9862328 |
| corrected / `39b70a103d99f422` | tiny-create-1 | verified.canonical-verification.canonical_InodeRecord_objects | 3 | 100636 | 100636 | 100636 |
| corrected / `39b70a103d99f422` | tiny-create-1 | verified.canonical-verification.canonical_InodeTable_bytes | 3 | 6564444 | 6563580 | 6566496 |
| corrected / `39b70a103d99f422` | tiny-create-1 | verified.canonical-verification.canonical_InodeTable_objects | 3 | 1141 | 1133 | 1160 |
| corrected / `39b70a103d99f422` | tiny-create-1 | verified.canonical-verification.canonical_Metadata_bytes | 3 | 286 | 286 | 286 |
| corrected / `39b70a103d99f422` | tiny-create-1 | verified.canonical-verification.canonical_Metadata_objects | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | tiny-create-1 | verified.canonical-verification.canonical_Namespace_bytes | 3 | 121 | 121 | 121 |
| corrected / `39b70a103d99f422` | tiny-create-1 | verified.canonical-verification.canonical_Namespace_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-1 | verified.canonical-verification.canonical_unique_bytes | 3 | 566823540 | 566823491 | 566824965 |
| corrected / `39b70a103d99f422` | tiny-create-1 | verified.canonical-verification.canonical_unique_objects | 3 | 411617 | 411597 | 411635 |
| corrected / `39b70a103d99f422` | tiny-create-1 | verified.canonical-verification.independent_content_paths | 3 | 100001 | 100001 | 100001 |
| corrected / `39b70a103d99f422` | tiny-create-1 | verified.canonical-verification.logical_bytes | 3 | 524288000 | 524288000 | 524288000 |
| corrected / `39b70a103d99f422` | tiny-create-1 | verified.canonical-verification.persistence_custody_paths | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | verified.canonical-verification.verified_paths | 3 | 100645 | 100645 | 100645 |
| corrected / `39b70a103d99f422` | tiny-create-1 | verified.canonical-verification.verified_regular_paths | 3 | 100001 | 100001 | 100001 |
| corrected / `39b70a103d99f422` | tiny-create-1 | visibility_ns | 3 | 104000 | 100708 | 128333 |
| corrected / `39b70a103d99f422` | tiny-create-1 | visited_file_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | visited_path_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | workload_chmod_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | workload_close_call_count | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | tiny-create-1 | workload_closedir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | workload_fsync_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | workload_fsyncdir_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-1 | workload_ftruncate_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | workload_lstat_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | workload_mkdir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | workload_ns | 3 | 5104209 | 4933667 | 7389958 |
| corrected / `39b70a103d99f422` | tiny-create-1 | workload_open_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-1 | workload_open_directory_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-1 | workload_opendir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | workload_plan_ns | 3 | 827750 | 810000 | 859750 |
| corrected / `39b70a103d99f422` | tiny-create-1 | workload_pread_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | workload_pwrite_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | workload_rename_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | workload_rmdir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | workload_symlink_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-1 | workload_unlink_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | attempted_syscall_count | 3 | 32 | 32 | 32 |
| corrected / `39b70a103d99f422` | tiny-create-10 | benchmark_injection_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | benchmark_reopen_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | benchmark_verifier_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | cache_acquisition_ns | 3 | 614713958 | 604489375 | 627254875 |
| corrected / `39b70a103d99f422` | tiny-create-10 | cache_build_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | cache_validation_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | candidate.admission_transactions | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | candidate.batch_inserted_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | candidate.batch_inserted_objects | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | candidate.candidate_bytes | 3 | 199168 | 195520 | 201324 |
| corrected / `39b70a103d99f422` | tiny-create-10 | candidate.candidate_objects | 3 | 109 | 109 | 110 |
| corrected / `39b70a103d99f422` | tiny-create-10 | candidate.final_inserted_bytes | 3 | 198587 | 194939 | 200743 |
| corrected / `39b70a103d99f422` | tiny-create-10 | candidate.final_inserted_objects | 3 | 102 | 102 | 103 |
| corrected / `39b70a103d99f422` | tiny-create-10 | candidate.inserted_bytes | 3 | 198587 | 194939 | 200743 |
| corrected / `39b70a103d99f422` | tiny-create-10 | candidate.inserted_objects | 3 | 102 | 102 | 103 |
| corrected / `39b70a103d99f422` | tiny-create-10 | candidate.max_transaction_bytes | 3 | 198587 | 194939 | 200743 |
| corrected / `39b70a103d99f422` | tiny-create-10 | candidate.max_transaction_objects | 3 | 102 | 102 | 103 |
| corrected / `39b70a103d99f422` | tiny-create-10 | candidate.preexisting_reused_bytes | 3 | 581 | 581 | 581 |
| corrected / `39b70a103d99f422` | tiny-create-10 | candidate.preexisting_reused_objects | 3 | 7 | 7 | 7 |
| corrected / `39b70a103d99f422` | tiny-create-10 | candidate.reused_bytes | 3 | 581 | 581 | 581 |
| corrected / `39b70a103d99f422` | tiny-create-10 | candidate.reused_objects | 3 | 7 | 7 | 7 |
| corrected / `39b70a103d99f422` | tiny-create-10 | cgroup.cpu_usage_usec_delta | 3 | 13999 | 13484 | 16487 |
| corrected / `39b70a103d99f422` | tiny-create-10 | cgroup.cpu_usage_usec_end | 3 | 61313 | 58717 | 66725 |
| corrected / `39b70a103d99f422` | tiny-create-10 | cgroup.cpu_usage_usec_start | 3 | 44826 | 44718 | 53241 |
| corrected / `39b70a103d99f422` | tiny-create-10 | cgroup.observed_max.memory.current | 3 | 4022272 | 3981312 | 4411392 |
| corrected / `39b70a103d99f422` | tiny-create-10 | cgroup.observed_max.memory.events.oom | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | cgroup.observed_max.memory.events.oom_kill | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | cgroup.observed_max.memory.peak | 3 | 4562944 | 4464640 | 4734976 |
| corrected / `39b70a103d99f422` | tiny-create-10 | cgroup.observed_max.memory.stat.anon | 3 | 2166784 | 2162688 | 2179072 |
| corrected / `39b70a103d99f422` | tiny-create-10 | cgroup.observed_max.memory.stat.file | 3 | 4096 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | tiny-create-10 | cgroup.observed_max.memory.stat.file_dirty | 3 | 4096 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | tiny-create-10 | cgroup.observed_max.memory.stat.file_writeback | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | cgroup.observed_max.memory.stat.kernel | 3 | 1323008 | 1314816 | 1327104 |
| corrected / `39b70a103d99f422` | tiny-create-10 | cgroup.observed_max.memory.stat.shmem | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | cgroup.observed_max.memory.stat.slab | 3 | 686808 | 686728 | 688808 |
| corrected / `39b70a103d99f422` | tiny-create-10 | cgroup.observed_max.memory.swap.current | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | cgroup.observed_max.pids.current | 3 | 17 | 17 | 17 |
| corrected / `39b70a103d99f422` | tiny-create-10 | cleanup_ns | 3 | 313580959 | 299652417 | 322355958 |
| corrected / `39b70a103d99f422` | tiny-create-10 | clone_bytes | 3 | 672202752 | 672137216 | 672595968 |
| corrected / `39b70a103d99f422` | tiny-create-10 | clone_wall_ns | 3 | 429295583 | 428447750 | 443140708 |
| corrected / `39b70a103d99f422` | tiny-create-10 | command_wall_ns | 3 | 1986457125 | 1986104125 | 2014503291 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_diagnostics.cdc_bytes_scanned | 3 | 16489 | 16489 | 16489 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_diagnostics.edit_count | 3 | 9 | 9 | 9 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_diagnostics.edit_metric_nodes_scanned | 3 | 9 | 9 | 9 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_diagnostics.edit_piece_count | 3 | 9 | 9 | 9 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_diagnostics.edit_piece_height | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_diagnostics.edit_piece_logical_charge | 3 | 72 | 72 | 72 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_diagnostics.edit_spool_allocated_bytes | 3 | 16489 | 16489 | 16489 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_diagnostics.edit_spool_live_bytes | 3 | 16489 | 16489 | 16489 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_diagnostics.edit_spool_peak_bytes | 3 | 16489 | 16489 | 16489 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_diagnostics.edit_spool_superseded_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_diagnostics.edit_tree_visits | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_diagnostics.namespace_base_paths_visited | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_diagnostics.namespace_candidate_probe_nodes | 3 | 23 | 23 | 24 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_diagnostics.namespace_clean_nodes_visited | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_diagnostics.namespace_dirty_nodes_visited | 3 | 20 | 20 | 20 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_diagnostics.namespace_final_paths_visited | 3 | 70 | 70 | 70 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_diagnostics.physical_spool_allocated_bytes | 3 | 40960 | 40960 | 40960 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_diagnostics.physical_spool_observation_count | 3 | 47 | 47 | 47 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_diagnostics.physical_spool_observation_errors | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_diagnostics.physical_spool_peak_bytes | 3 | 40960 | 40960 | 40960 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_ns | 3 | 20284542 | 15554042 | 31365708 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.candidate_finish_ns | 3 | 631875 | 611208 | 711125 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.candidate_plan_ns | 3 | 500 | 500 | 584 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.capture_ns | 3 | 9250 | 5791 | 9500 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.captured_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.captured_files | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.content_ns | 3 | 1110000 | 1108583 | 1247708 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.dirty_compare_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.in_place_rebase_ns | 3 | 4317792 | 4063125 | 4894417 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.local_admission_ns | 3 | 1229958 | 1104500 | 1237500 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.max_admission_transaction_bytes | 3 | 198587 | 194939 | 200743 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.max_admission_transaction_objects | 3 | 102 | 102 | 103 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.namespace_ns | 3 | 2820125 | 2746167 | 2887416 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.object_admission_begin_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.object_admission_commit_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.object_admission_insert_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.object_admission_ns | 3 | 31417 | 30959 | 35584 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.object_admission_transactions | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.pause_fence_ns | 3 | 373458 | 296584 | 558166 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.payload_bytes_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.publication_begin_ns | 3 | 4792 | 4583 | 5333 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.publication_commit_ns | 3 | 7591083 | 2071250 | 18106208 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.publication_insert_ns | 3 | 1357959 | 1292746 | 1376336 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.publication_metadata_ns | 3 | 76917 | 76208 | 81833 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.publication_ns | 3 | 9061084 | 3526791 | 19491208 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.publication_payload_ns | 3 | 2453 | 2083 | 2457 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.quiesce_ns | 3 | 250 | 250 | 458 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.resume_ns | 3 | 376917 | 364666 | 743875 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.snapshot_database_bytes | 3 | 754363 | 739235 | 759651 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.snapshot_database_calls | 3 | 221 | 221 | 223 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.snapshot_database_rows | 3 | 221 | 221 | 223 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.total_ns | 3 | 20280958 | 15549708 | 31361833 |
| corrected / `39b70a103d99f422` | tiny-create-10 | commit_work.unattributed_ns | 3 | 487793 | 474833 | 590582 |
| corrected / `39b70a103d99f422` | tiny-create-10 | completed_chain_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | completed_episode_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | completed_file_write_count | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | tiny-create-10 | completed_read_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | completed_read_request_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | completed_syscall_count | 3 | 32 | 32 | 32 |
| corrected / `39b70a103d99f422` | tiny-create-10 | completed_target_count | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | tiny-create-10 | completed_write_bytes | 3 | 16489 | 16489 | 16489 |
| corrected / `39b70a103d99f422` | tiny-create-10 | create_ns | 3 | 10388417 | 10136458 | 11617042 |
| corrected / `39b70a103d99f422` | tiny-create-10 | created_commit_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-10 | directory_entry_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | editor_save_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | end_ns | 3 | 2965250 | 2931750 | 3124583 |
| corrected / `39b70a103d99f422` | tiny-create-10 | exec_ns | 3 | 36570500 | 34077792 | 37104000 |
| corrected / `39b70a103d99f422` | tiny-create-10 | external_process_wall_ns | 3 | 157072833 | 155965083 | 162715667 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.callback_access | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.callback_create | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.callback_flush | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.callback_fsync | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.callback_fsyncdir | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.callback_getattr | 3 | 11 | 11 | 11 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.callback_link | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.callback_lookup | 3 | 31 | 31 | 31 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.callback_mkdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.callback_mknod | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.callback_open | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.callback_opendir | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.callback_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.callback_readdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.callback_readdirplus | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.callback_readlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.callback_release | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.callback_releasedir | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.callback_rename | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.callback_rmdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.callback_setattr | 3 | 40 | 40 | 40 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.callback_statfs | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.callback_symlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.callback_unlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.callback_write | 3 | 9 | 9 | 9 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.client_decode_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.client_decode_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.client_response_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.client_response_frames | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.client_socket_read_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.collection_ns | 3 | 538167 | 439834 | 582625 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.directory_entries_returned | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.directory_nonzero_offset_requests | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.host_dispatch_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.host_encode_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.host_response_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.host_response_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.host_response_frames | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.host_socket_write_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.init_capabilities | 3 | 4481057 | 4481057 | 4481057 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.kernel_read_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.kernel_read_gt_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.kernel_read_le_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.kernel_read_le_256k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.kernel_read_le_4k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.kernel_read_le_64k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.kernel_read_requests | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.local_bytes | 3 | 952261 | 936557 | 962989 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.local_calls | 3 | 903 | 903 | 905 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.local_ids | 3 | 903 | 903 | 905 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.local_read_auth_ns | 3 | 6962438 | 6663189 | 7143455 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.local_rows | 3 | 903 | 903 | 905 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.max_payload_batch | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.max_readahead_bytes | 3 | 131072 | 131072 | 131072 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.payload_batches | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.payload_bytes_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.payload_ids | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.read_ahead_cache_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.read_ahead_fetched_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.read_ahead_fetches | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.read_ahead_hits | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.read_ahead_misses | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.read_ahead_requested_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.read_ahead_served_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.read_ahead_unused_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.read_plan_builds | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.rope_nodes_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.snapshot_cache_bytes | 3 | 71622 | 71622 | 71622 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.snapshot_cache_hits | 3 | 641 | 641 | 641 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.snapshot_cache_rows | 3 | 641 | 641 | 641 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.snapshot_database_bytes | 3 | 880639 | 864935 | 891367 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.snapshot_database_calls | 3 | 262 | 262 | 264 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.snapshot_database_rows | 3 | 262 | 262 | 264 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.workspace_output_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.workspace_read_calls | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.workspace_read_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_read.workspace_requested_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_write.client_frame_bytes | 3 | 16714 | 16714 | 16714 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_write.client_request_copy_bytes | 3 | 16489 | 16489 | 16489 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_write.collection_ns | 3 | 362083 | 341875 | 459542 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_write.decode_ns | 3 | 6251 | 5458 | 8335 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_write.encode_ns | 3 | 376 | 375 | 460 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_write.frame_payload_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_write.host_decode_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_write.host_dispatch_ns | 3 | 540666 | 497835 | 551626 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_write.host_frame_bytes | 3 | 16714 | 16714 | 16714 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_write.kernel_write_bytes | 3 | 16489 | 16489 | 16489 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_write.kernel_write_gt_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_write.kernel_write_le_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_write.kernel_write_le_256k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_write.kernel_write_le_4k | 3 | 8 | 8 | 8 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_write.kernel_write_le_64k | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_write.kernel_write_requests | 3 | 9 | 9 | 9 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_write.max_write_bytes | 3 | 1048576 | 1048576 | 1048576 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_write.socket_read_ns | 3 | 3315082 | 3166000 | 3959916 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_write.socket_write_ns | 3 | 158877 | 156376 | 183665 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_write.spool_write_bytes | 3 | 16489 | 16489 | 16489 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_write.spool_write_ns | 3 | 1668124 | 1567628 | 1795584 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_write.spool_write_open_count | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_write.workspace_fence_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-10 | fuse_write.workspace_fence_ns | 3 | 65083 | 64083 | 66084 |
| corrected / `39b70a103d99f422` | tiny-create-10 | git_process_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | host.disk_read_bytes.delta | 3 | 0 | 0 | 16384 |
| corrected / `39b70a103d99f422` | tiny-create-10 | host.disk_read_bytes.end | 3 | 0 | 0 | 16384 |
| corrected / `39b70a103d99f422` | tiny-create-10 | host.disk_read_bytes.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | host.disk_write_bytes.delta | 3 | 53248 | 49152 | 53248 |
| corrected / `39b70a103d99f422` | tiny-create-10 | host.disk_write_bytes.end | 3 | 53248 | 49152 | 53248 |
| corrected / `39b70a103d99f422` | tiny-create-10 | host.disk_write_bytes.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | host.peak_resident_bytes.max | 3 | 27738112 | 27443200 | 28344320 |
| corrected / `39b70a103d99f422` | tiny-create-10 | host.physical_footprint_bytes.max | 3 | 11354592 | 11239904 | 11551200 |
| corrected / `39b70a103d99f422` | tiny-create-10 | host.resident_bytes.max | 3 | 27688960 | 27410432 | 28295168 |
| corrected / `39b70a103d99f422` | tiny-create-10 | host.swaps.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | host.swaps.end | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | host.swaps.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | host.system_cpu_ns.delta | 3 | 42925042 | 39779334 | 50591334 |
| corrected / `39b70a103d99f422` | tiny-create-10 | host.system_cpu_ns.end | 3 | 45005958 | 41626625 | 52095500 |
| corrected / `39b70a103d99f422` | tiny-create-10 | host.system_cpu_ns.start | 3 | 1847291 | 1504166 | 2080916 |
| corrected / `39b70a103d99f422` | tiny-create-10 | host.user_cpu_ns.delta | 3 | 20859917 | 20714291 | 21074083 |
| corrected / `39b70a103d99f422` | tiny-create-10 | host.user_cpu_ns.end | 3 | 22762125 | 22524041 | 23055166 |
| corrected / `39b70a103d99f422` | tiny-create-10 | host.user_cpu_ns.start | 3 | 1902208 | 1809750 | 1981083 |
| corrected / `39b70a103d99f422` | tiny-create-10 | host_orchestration_ns | 3 | 100576917 | 98494750 | 108193208 |
| corrected / `39b70a103d99f422` | tiny-create-10 | host_sampler.baseline_bytes | 3 | 2605056 | 2572288 | 2637824 |
| corrected / `39b70a103d99f422` | tiny-create-10 | host_sampler.final_bytes | 3 | 14499840 | 14368768 | 14696448 |
| corrected / `39b70a103d99f422` | tiny-create-10 | host_sampler.maximum_gap_ns | 3 | 12522792 | 12519250 | 12530542 |
| corrected / `39b70a103d99f422` | tiny-create-10 | host_sampler.sample_count | 3 | 13 | 13 | 14 |
| corrected / `39b70a103d99f422` | tiny-create-10 | host_sampler.sampled_peak_bytes | 3 | 27705344 | 27410432 | 28311552 |
| corrected / `39b70a103d99f422` | tiny-create-10 | inplace_edit_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | input.fixture_bytes | 3 | 524288000 | 524288000 | 524288000 |
| corrected / `39b70a103d99f422` | tiny-create-10 | input.regular_files | 3 | 100000 | 100000 | 100000 |
| corrected / `39b70a103d99f422` | tiny-create-10 | interrupted_syscall_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | lifecycle.anchor_prefetch_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | lifecycle.cleanup_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | lifecycle.docker_calls | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | lifecycle.docker_setup_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | lifecycle.helper_copy_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | lifecycle.mount_ready_ns | 3 | 8969417 | 8751541 | 9960208 |
| corrected / `39b70a103d99f422` | tiny-create-10 | lifecycle.proxy_ns | 3 | 170708 | 167708 | 198375 |
| corrected / `39b70a103d99f422` | tiny-create-10 | lifecycle.small_file_prefetch_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | lifecycle.small_file_prefetch_eligible | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | lifecycle.snapshot_cache_bytes_at_create | 3 | 1376 | 1376 | 1376 |
| corrected / `39b70a103d99f422` | tiny-create-10 | lifecycle.snapshot_cache_rows_at_create | 3 | 9 | 9 | 9 |
| corrected / `39b70a103d99f422` | tiny-create-10 | lifecycle.snapshot_database_bytes | 3 | 11684 | 10852 | 11876 |
| corrected / `39b70a103d99f422` | tiny-create-10 | lifecycle.snapshot_database_calls | 3 | 12 | 12 | 12 |
| corrected / `39b70a103d99f422` | tiny-create-10 | lifecycle.snapshot_database_rows | 3 | 12 | 12 | 12 |
| corrected / `39b70a103d99f422` | tiny-create-10 | lifecycle.snapshot_store_wide_scans | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | lifecycle.total_ns | 3 | 11043292 | 10766875 | 11871251 |
| corrected / `39b70a103d99f422` | tiny-create-10 | lifecycle.unattributed_ns | 3 | 253168 | 228125 | 323002 |
| corrected / `39b70a103d99f422` | tiny-create-10 | lifecycle.unmount_ns | 3 | 779542 | 656583 | 815083 |
| corrected / `39b70a103d99f422` | tiny-create-10 | lifecycle.wait_ns | 3 | 779375 | 760750 | 867833 |
| corrected / `39b70a103d99f422` | tiny-create-10 | metadata_normalization_count | 3 | 20 | 20 | 20 |
| corrected / `39b70a103d99f422` | tiny-create-10 | metadata_normalization_ns | 3 | 14817792 | 14525458 | 15857167 |
| corrected / `39b70a103d99f422` | tiny-create-10 | orchestration_unattributed_ns | 3 | 29587209 | 29549667 | 31740583 |
| corrected / `39b70a103d99f422` | tiny-create-10 | preparation_ns | 3 | 1528491333 | 1516113291 | 1528513166 |
| corrected / `39b70a103d99f422` | tiny-create-10 | pure_call_sum_ns | 3 | 70989708 | 66754167 | 78643541 |
| corrected / `39b70a103d99f422` | tiny-create-10 | root_sync_ns | 3 | 449708 | 409833 | 481667 |
| corrected / `39b70a103d99f422` | tiny-create-10 | runtime_preparation_ns | 3 | 403738583 | 352464208 | 404670917 |
| corrected / `39b70a103d99f422` | tiny-create-10 | spool_boundary.max_allocated_bytes | 3 | 45056 | 45056 | 45056 |
| corrected / `39b70a103d99f422` | tiny-create-10 | spool_boundary.max_file_count | 3 | 11 | 11 | 11 |
| corrected / `39b70a103d99f422` | tiny-create-10 | spool_boundary.max_logical_bytes | 3 | 16631 | 16631 | 16631 |
| corrected / `39b70a103d99f422` | tiny-create-10 | store.allocated_bytes.delta | 3 | 16777216 | 16777216 | 16777216 |
| corrected / `39b70a103d99f422` | tiny-create-10 | store.allocated_bytes.end | 3 | 688979968 | 688914432 | 689373184 |
| corrected / `39b70a103d99f422` | tiny-create-10 | store.allocated_bytes.max | 3 | 688979968 | 688914432 | 689373184 |
| corrected / `39b70a103d99f422` | tiny-create-10 | store.allocated_bytes.start | 3 | 672202752 | 672137216 | 672595968 |
| corrected / `39b70a103d99f422` | tiny-create-10 | store.file_bytes.delta | 3 | 196608 | 196608 | 196608 |
| corrected / `39b70a103d99f422` | tiny-create-10 | store.file_bytes.end | 3 | 672399360 | 672333824 | 672792576 |
| corrected / `39b70a103d99f422` | tiny-create-10 | store.file_bytes.max | 3 | 672399360 | 672333824 | 672792576 |
| corrected / `39b70a103d99f422` | tiny-create-10 | store.file_bytes.start | 3 | 672202752 | 672137216 | 672595968 |
| corrected / `39b70a103d99f422` | tiny-create-10 | store.freelist_page_count.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | store.freelist_page_count.end | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | store.freelist_page_count.max | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | store.freelist_page_count.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | store.live_page_bytes.delta | 3 | 196608 | 196608 | 196608 |
| corrected / `39b70a103d99f422` | tiny-create-10 | store.live_page_bytes.end | 3 | 672399360 | 672333824 | 672792576 |
| corrected / `39b70a103d99f422` | tiny-create-10 | store.live_page_bytes.max | 3 | 672399360 | 672333824 | 672792576 |
| corrected / `39b70a103d99f422` | tiny-create-10 | store.live_page_bytes.start | 3 | 672202752 | 672137216 | 672595968 |
| corrected / `39b70a103d99f422` | tiny-create-10 | store.page_count.delta | 3 | 3 | 3 | 3 |
| corrected / `39b70a103d99f422` | tiny-create-10 | store.page_count.end | 3 | 10260 | 10259 | 10266 |
| corrected / `39b70a103d99f422` | tiny-create-10 | store.page_count.max | 3 | 10260 | 10259 | 10266 |
| corrected / `39b70a103d99f422` | tiny-create-10 | store.page_count.start | 3 | 10257 | 10256 | 10263 |
| corrected / `39b70a103d99f422` | tiny-create-10 | verified.canonical-verification.canonical_Chunk_bytes | 3 | 526576625 | 526575638 | 526576835 |
| corrected / `39b70a103d99f422` | tiny-create-10 | verified.canonical-verification.canonical_Chunk_objects | 3 | 108196 | 108149 | 108206 |
| corrected / `39b70a103d99f422` | tiny-create-10 | verified.canonical-verification.canonical_DirectoryNode_bytes | 3 | 4447693 | 4447693 | 4447693 |
| corrected / `39b70a103d99f422` | tiny-create-10 | verified.canonical-verification.canonical_DirectoryNode_objects | 3 | 1024 | 1024 | 1024 |
| corrected / `39b70a103d99f422` | tiny-create-10 | verified.canonical-verification.canonical_DirectoryState_bytes | 3 | 63112 | 63112 | 63112 |
| corrected / `39b70a103d99f422` | tiny-create-10 | verified.canonical-verification.canonical_DirectoryState_objects | 3 | 644 | 644 | 644 |
| corrected / `39b70a103d99f422` | tiny-create-10 | verified.canonical-verification.canonical_FileNode_bytes | 3 | 8728412 | 8726532 | 8728812 |
| corrected / `39b70a103d99f422` | tiny-create-10 | verified.canonical-verification.canonical_FileNode_objects | 3 | 100013 | 100013 | 100013 |
| corrected / `39b70a103d99f422` | tiny-create-10 | verified.canonical-verification.canonical_FileState_bytes | 3 | 10601378 | 10601378 | 10601378 |
| corrected / `39b70a103d99f422` | tiny-create-10 | verified.canonical-verification.canonical_FileState_objects | 3 | 100013 | 100013 | 100013 |
| corrected / `39b70a103d99f422` | tiny-create-10 | verified.canonical-verification.canonical_InodeRecord_bytes | 3 | 9864092 | 9864092 | 9864092 |
| corrected / `39b70a103d99f422` | tiny-create-10 | verified.canonical-verification.canonical_InodeRecord_objects | 3 | 100654 | 100654 | 100654 |
| corrected / `39b70a103d99f422` | tiny-create-10 | verified.canonical-verification.canonical_InodeTable_bytes | 3 | 6565020 | 6564156 | 6567072 |
| corrected / `39b70a103d99f422` | tiny-create-10 | verified.canonical-verification.canonical_InodeTable_objects | 3 | 1141 | 1133 | 1160 |
| corrected / `39b70a103d99f422` | tiny-create-10 | verified.canonical-verification.canonical_Metadata_bytes | 3 | 286 | 286 | 286 |
| corrected / `39b70a103d99f422` | tiny-create-10 | verified.canonical-verification.canonical_Metadata_objects | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | tiny-create-10 | verified.canonical-verification.canonical_Namespace_bytes | 3 | 121 | 121 | 121 |
| corrected / `39b70a103d99f422` | tiny-create-10 | verified.canonical-verification.canonical_Namespace_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-10 | verified.canonical-verification.canonical_unique_bytes | 3 | 566845924 | 566845875 | 566847349 |
| corrected / `39b70a103d99f422` | tiny-create-10 | verified.canonical-verification.canonical_unique_objects | 3 | 411680 | 411660 | 411698 |
| corrected / `39b70a103d99f422` | tiny-create-10 | verified.canonical-verification.independent_content_paths | 3 | 100010 | 100010 | 100010 |
| corrected / `39b70a103d99f422` | tiny-create-10 | verified.canonical-verification.logical_bytes | 3 | 524304489 | 524304489 | 524304489 |
| corrected / `39b70a103d99f422` | tiny-create-10 | verified.canonical-verification.persistence_custody_paths | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | verified.canonical-verification.verified_paths | 3 | 100654 | 100654 | 100654 |
| corrected / `39b70a103d99f422` | tiny-create-10 | verified.canonical-verification.verified_regular_paths | 3 | 100010 | 100010 | 100010 |
| corrected / `39b70a103d99f422` | tiny-create-10 | visibility_ns | 3 | 88166 | 80833 | 98333 |
| corrected / `39b70a103d99f422` | tiny-create-10 | visited_file_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | visited_path_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | workload_chmod_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | workload_close_call_count | 3 | 11 | 11 | 11 |
| corrected / `39b70a103d99f422` | tiny-create-10 | workload_closedir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | workload_fsync_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | workload_fsyncdir_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-10 | workload_ftruncate_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | workload_lstat_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | workload_mkdir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | workload_ns | 3 | 31195375 | 29260708 | 31657834 |
| corrected / `39b70a103d99f422` | tiny-create-10 | workload_open_call_count | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | tiny-create-10 | workload_open_directory_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-10 | workload_opendir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | workload_plan_ns | 3 | 830500 | 806666 | 964417 |
| corrected / `39b70a103d99f422` | tiny-create-10 | workload_pread_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | workload_pwrite_call_count | 3 | 9 | 9 | 9 |
| corrected / `39b70a103d99f422` | tiny-create-10 | workload_rename_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | workload_rmdir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | workload_symlink_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-10 | workload_unlink_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | attempted_syscall_count | 3 | 293 | 293 | 293 |
| corrected / `39b70a103d99f422` | tiny-create-100 | benchmark_injection_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | benchmark_reopen_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | benchmark_verifier_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | cache_acquisition_ns | 3 | 634276958 | 600469000 | 659046500 |
| corrected / `39b70a103d99f422` | tiny-create-100 | cache_build_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | cache_validation_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | candidate.admission_transactions | 3 | 4 | 4 | 4 |
| corrected / `39b70a103d99f422` | tiny-create-100 | candidate.batch_inserted_bytes | 3 | 868096 | 858816 | 890042 |
| corrected / `39b70a103d99f422` | tiny-create-100 | candidate.batch_inserted_objects | 3 | 508 | 508 | 508 |
| corrected / `39b70a103d99f422` | tiny-create-100 | candidate.candidate_bytes | 3 | 902076 | 885766 | 929250 |
| corrected / `39b70a103d99f422` | tiny-create-100 | candidate.candidate_objects | 3 | 522 | 519 | 527 |
| corrected / `39b70a103d99f422` | tiny-create-100 | candidate.final_inserted_bytes | 3 | 26369 | 11453 | 60573 |
| corrected / `39b70a103d99f422` | tiny-create-100 | candidate.final_inserted_objects | 3 | 7 | 4 | 12 |
| corrected / `39b70a103d99f422` | tiny-create-100 | candidate.inserted_bytes | 3 | 901495 | 885185 | 928669 |
| corrected / `39b70a103d99f422` | tiny-create-100 | candidate.inserted_objects | 3 | 515 | 512 | 520 |
| corrected / `39b70a103d99f422` | tiny-create-100 | candidate.max_transaction_bytes | 3 | 669724 | 660444 | 691588 |
| corrected / `39b70a103d99f422` | tiny-create-100 | candidate.max_transaction_objects | 3 | 127 | 127 | 127 |
| corrected / `39b70a103d99f422` | tiny-create-100 | candidate.preexisting_reused_bytes | 3 | 581 | 581 | 581 |
| corrected / `39b70a103d99f422` | tiny-create-100 | candidate.preexisting_reused_objects | 3 | 7 | 7 | 7 |
| corrected / `39b70a103d99f422` | tiny-create-100 | candidate.reused_bytes | 3 | 581 | 581 | 581 |
| corrected / `39b70a103d99f422` | tiny-create-100 | candidate.reused_objects | 3 | 7 | 7 | 7 |
| corrected / `39b70a103d99f422` | tiny-create-100 | cgroup.cpu_usage_usec_delta | 3 | 47618 | 43550 | 51780 |
| corrected / `39b70a103d99f422` | tiny-create-100 | cgroup.cpu_usage_usec_end | 3 | 90751 | 87543 | 95217 |
| corrected / `39b70a103d99f422` | tiny-create-100 | cgroup.cpu_usage_usec_start | 3 | 43437 | 43133 | 43993 |
| corrected / `39b70a103d99f422` | tiny-create-100 | cgroup.observed_max.memory.current | 3 | 4366336 | 4149248 | 4415488 |
| corrected / `39b70a103d99f422` | tiny-create-100 | cgroup.observed_max.memory.events.oom | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | cgroup.observed_max.memory.events.oom_kill | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | cgroup.observed_max.memory.peak | 3 | 4788224 | 4419584 | 5365760 |
| corrected / `39b70a103d99f422` | tiny-create-100 | cgroup.observed_max.memory.stat.anon | 3 | 2199552 | 2199552 | 2199552 |
| corrected / `39b70a103d99f422` | tiny-create-100 | cgroup.observed_max.memory.stat.file | 3 | 4096 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | tiny-create-100 | cgroup.observed_max.memory.stat.file_dirty | 3 | 4096 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | tiny-create-100 | cgroup.observed_max.memory.stat.file_writeback | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | cgroup.observed_max.memory.stat.kernel | 3 | 1425408 | 1425408 | 1429504 |
| corrected / `39b70a103d99f422` | tiny-create-100 | cgroup.observed_max.memory.stat.shmem | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | cgroup.observed_max.memory.stat.slab | 3 | 793616 | 793616 | 793704 |
| corrected / `39b70a103d99f422` | tiny-create-100 | cgroup.observed_max.memory.swap.current | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | cgroup.observed_max.pids.current | 3 | 17 | 17 | 17 |
| corrected / `39b70a103d99f422` | tiny-create-100 | cleanup_ns | 3 | 270589041 | 261910417 | 277423125 |
| corrected / `39b70a103d99f422` | tiny-create-100 | clone_bytes | 3 | 672202752 | 672137216 | 672595968 |
| corrected / `39b70a103d99f422` | tiny-create-100 | clone_wall_ns | 3 | 451082791 | 425178375 | 476743625 |
| corrected / `39b70a103d99f422` | tiny-create-100 | command_wall_ns | 3 | 2191959791 | 2182730625 | 2278410958 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_diagnostics.cdc_bytes_scanned | 3 | 164890 | 164890 | 164890 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_diagnostics.edit_count | 3 | 90 | 90 | 90 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_diagnostics.edit_metric_nodes_scanned | 3 | 90 | 90 | 90 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_diagnostics.edit_piece_count | 3 | 90 | 90 | 90 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_diagnostics.edit_piece_height | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_diagnostics.edit_piece_logical_charge | 3 | 720 | 720 | 720 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_diagnostics.edit_spool_allocated_bytes | 3 | 164890 | 164890 | 164890 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_diagnostics.edit_spool_live_bytes | 3 | 164890 | 164890 | 164890 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_diagnostics.edit_spool_peak_bytes | 3 | 164890 | 164890 | 164890 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_diagnostics.edit_spool_superseded_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_diagnostics.edit_tree_visits | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_diagnostics.namespace_base_paths_visited | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_diagnostics.namespace_candidate_probe_nodes | 3 | 118 | 117 | 133 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_diagnostics.namespace_clean_nodes_visited | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_diagnostics.namespace_dirty_nodes_visited | 3 | 110 | 110 | 110 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_diagnostics.namespace_final_paths_visited | 3 | 700 | 700 | 700 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_diagnostics.physical_spool_allocated_bytes | 3 | 409600 | 409600 | 409600 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_diagnostics.physical_spool_observation_count | 3 | 470 | 470 | 470 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_diagnostics.physical_spool_observation_errors | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_diagnostics.physical_spool_peak_bytes | 3 | 409600 | 409600 | 409600 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_ns | 3 | 67181709 | 59576333 | 71894792 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.candidate_finish_ns | 3 | 3011459 | 2940292 | 3225791 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.candidate_plan_ns | 3 | 542 | 500 | 792 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.capture_ns | 3 | 6542 | 5959 | 7708 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.captured_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.captured_files | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.content_ns | 3 | 3412042 | 3296250 | 4140500 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.dirty_compare_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.in_place_rebase_ns | 3 | 21700459 | 20807583 | 21897625 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.local_admission_ns | 3 | 3036208 | 2919625 | 3421458 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.max_admission_transaction_bytes | 3 | 669724 | 660444 | 691588 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.max_admission_transaction_objects | 3 | 127 | 127 | 127 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.namespace_ns | 3 | 13634291 | 13563542 | 14430750 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.object_admission_begin_ns | 3 | 25957 | 25790 | 36917 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.object_admission_commit_ns | 3 | 12660208 | 7336874 | 20337042 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.object_admission_insert_ns | 3 | 5605448 | 5405148 | 5623558 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.object_admission_ns | 3 | 18370042 | 13255292 | 26260625 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.object_admission_transactions | 3 | 4 | 4 | 4 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.pause_fence_ns | 3 | 347167 | 279542 | 533833 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.payload_bytes_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.publication_begin_ns | 3 | 7584 | 5667 | 7667 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.publication_commit_ns | 3 | 125125 | 75667 | 170916 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.publication_insert_ns | 3 | 106707 | 59084 | 167124 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.publication_metadata_ns | 3 | 93458 | 91125 | 96083 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.publication_ns | 3 | 334792 | 233042 | 444167 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.publication_payload_ns | 3 | 209 | 82 | 209 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.quiesce_ns | 3 | 250 | 209 | 250 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.resume_ns | 3 | 383000 | 361791 | 405083 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.snapshot_database_bytes | 3 | 2853503 | 2814067 | 2918843 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.snapshot_database_calls | 3 | 831 | 831 | 837 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.snapshot_database_rows | 3 | 831 | 831 | 837 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.total_ns | 3 | 67178000 | 59572375 | 71890375 |
| corrected / `39b70a103d99f422` | tiny-create-100 | commit_work.unattributed_ns | 3 | 565624 | 558998 | 847125 |
| corrected / `39b70a103d99f422` | tiny-create-100 | completed_chain_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | completed_episode_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | completed_file_write_count | 3 | 100 | 100 | 100 |
| corrected / `39b70a103d99f422` | tiny-create-100 | completed_read_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | completed_read_request_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | completed_syscall_count | 3 | 293 | 293 | 293 |
| corrected / `39b70a103d99f422` | tiny-create-100 | completed_target_count | 3 | 100 | 100 | 100 |
| corrected / `39b70a103d99f422` | tiny-create-100 | completed_write_bytes | 3 | 164890 | 164890 | 164890 |
| corrected / `39b70a103d99f422` | tiny-create-100 | create_ns | 3 | 10354042 | 8736250 | 13906708 |
| corrected / `39b70a103d99f422` | tiny-create-100 | created_commit_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-100 | directory_entry_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | editor_save_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | end_ns | 3 | 3313292 | 3165500 | 3816208 |
| corrected / `39b70a103d99f422` | tiny-create-100 | exec_ns | 3 | 191117125 | 188783833 | 192508833 |
| corrected / `39b70a103d99f422` | tiny-create-100 | external_process_wall_ns | 3 | 419180125 | 415044250 | 422633917 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.callback_access | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.callback_create | 3 | 100 | 100 | 100 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.callback_flush | 3 | 100 | 100 | 100 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.callback_fsync | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.callback_fsyncdir | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.callback_getattr | 3 | 101 | 101 | 101 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.callback_link | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.callback_lookup | 3 | 121 | 121 | 121 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.callback_mkdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.callback_mknod | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.callback_open | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.callback_opendir | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.callback_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.callback_readdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.callback_readdirplus | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.callback_readlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.callback_release | 3 | 100 | 100 | 100 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.callback_releasedir | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.callback_rename | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.callback_rmdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.callback_setattr | 3 | 220 | 220 | 220 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.callback_statfs | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.callback_symlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.callback_unlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.callback_write | 3 | 90 | 90 | 90 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.client_decode_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.client_decode_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.client_response_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.client_response_frames | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.client_socket_read_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.collection_ns | 3 | 579667 | 509667 | 596166 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.directory_entries_returned | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.directory_nonzero_offset_requests | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.host_dispatch_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.host_encode_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.host_response_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.host_response_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.host_response_frames | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.host_socket_write_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.init_capabilities | 3 | 4481057 | 4481057 | 4481057 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.kernel_read_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.kernel_read_gt_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.kernel_read_le_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.kernel_read_le_256k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.kernel_read_le_4k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.kernel_read_le_64k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.kernel_read_requests | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.local_bytes | 3 | 3295145 | 3261521 | 3360857 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.local_calls | 3 | 3513 | 3511 | 3517 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.local_ids | 3 | 3513 | 3511 | 3517 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.local_read_auth_ns | 3 | 21992039 | 21619337 | 22589793 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.local_rows | 3 | 3513 | 3511 | 3517 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.max_payload_batch | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.max_readahead_bytes | 3 | 131072 | 131072 | 131072 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.payload_batches | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.payload_bytes_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.payload_ids | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.read_ahead_cache_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.read_ahead_fetched_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.read_ahead_fetches | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.read_ahead_hits | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.read_ahead_misses | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.read_ahead_requested_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.read_ahead_served_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.read_ahead_unused_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.read_plan_builds | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.rope_nodes_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.snapshot_cache_bytes | 3 | 315738 | 315738 | 315942 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.snapshot_cache_hits | 3 | 2639 | 2639 | 2641 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.snapshot_cache_rows | 3 | 2639 | 2639 | 2641 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.snapshot_database_bytes | 3 | 2979203 | 2945783 | 3045119 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.snapshot_database_calls | 3 | 872 | 872 | 878 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.snapshot_database_rows | 3 | 872 | 872 | 878 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.workspace_output_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.workspace_read_calls | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.workspace_read_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_read.workspace_requested_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_write.client_frame_bytes | 3 | 167140 | 167140 | 167140 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_write.client_request_copy_bytes | 3 | 164890 | 164890 | 164890 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_write.collection_ns | 3 | 379417 | 377042 | 662167 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_write.decode_ns | 3 | 83373 | 81288 | 97251 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_write.encode_ns | 3 | 3797 | 3706 | 3915 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_write.frame_payload_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_write.host_decode_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_write.host_dispatch_ns | 3 | 4391786 | 4122036 | 4426827 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_write.host_frame_bytes | 3 | 167140 | 167140 | 167140 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_write.kernel_write_bytes | 3 | 164890 | 164890 | 164890 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_write.kernel_write_gt_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_write.kernel_write_le_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_write.kernel_write_le_256k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_write.kernel_write_le_4k | 3 | 80 | 80 | 80 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_write.kernel_write_le_64k | 3 | 10 | 10 | 10 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_write.kernel_write_requests | 3 | 90 | 90 | 90 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_write.max_write_bytes | 3 | 1048576 | 1048576 | 1048576 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_write.socket_read_ns | 3 | 43587660 | 43245096 | 44316638 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_write.socket_write_ns | 3 | 1829908 | 1717327 | 1852871 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_write.spool_write_bytes | 3 | 164890 | 164890 | 164890 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_write.spool_write_ns | 3 | 14439830 | 13645007 | 15066586 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_write.spool_write_open_count | 3 | 100 | 100 | 100 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_write.workspace_fence_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-100 | fuse_write.workspace_fence_ns | 3 | 419250 | 400417 | 430291 |
| corrected / `39b70a103d99f422` | tiny-create-100 | git_process_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | host.disk_read_bytes.delta | 3 | 0 | 0 | 4096 |
| corrected / `39b70a103d99f422` | tiny-create-100 | host.disk_read_bytes.end | 3 | 0 | 0 | 4096 |
| corrected / `39b70a103d99f422` | tiny-create-100 | host.disk_read_bytes.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | host.disk_write_bytes.delta | 3 | 421888 | 421888 | 421888 |
| corrected / `39b70a103d99f422` | tiny-create-100 | host.disk_write_bytes.end | 3 | 421888 | 421888 | 421888 |
| corrected / `39b70a103d99f422` | tiny-create-100 | host.disk_write_bytes.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | host.peak_resident_bytes.max | 3 | 48988160 | 48693248 | 49594368 |
| corrected / `39b70a103d99f422` | tiny-create-100 | host.physical_footprint_bytes.max | 3 | 16630216 | 16499168 | 16744928 |
| corrected / `39b70a103d99f422` | tiny-create-100 | host.resident_bytes.max | 3 | 48955392 | 48660480 | 49561600 |
| corrected / `39b70a103d99f422` | tiny-create-100 | host.swaps.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | host.swaps.end | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | host.swaps.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | host.system_cpu_ns.delta | 3 | 83884042 | 75807083 | 87262125 |
| corrected / `39b70a103d99f422` | tiny-create-100 | host.system_cpu_ns.end | 3 | 85456500 | 77505916 | 88897416 |
| corrected / `39b70a103d99f422` | tiny-create-100 | host.system_cpu_ns.start | 3 | 1635291 | 1572458 | 1698833 |
| corrected / `39b70a103d99f422` | tiny-create-100 | host.user_cpu_ns.delta | 3 | 51791042 | 51773084 | 52673250 |
| corrected / `39b70a103d99f422` | tiny-create-100 | host.user_cpu_ns.end | 3 | 53633958 | 53574000 | 54482375 |
| corrected / `39b70a103d99f422` | tiny-create-100 | host.user_cpu_ns.start | 3 | 1809125 | 1800916 | 1842916 |
| corrected / `39b70a103d99f422` | tiny-create-100 | host_orchestration_ns | 3 | 303775333 | 294530916 | 308150000 |
| corrected / `39b70a103d99f422` | tiny-create-100 | host_sampler.baseline_bytes | 3 | 2605056 | 2605056 | 2605056 |
| corrected / `39b70a103d99f422` | tiny-create-100 | host_sampler.final_bytes | 3 | 19775488 | 19644416 | 19890176 |
| corrected / `39b70a103d99f422` | tiny-create-100 | host_sampler.maximum_gap_ns | 3 | 12529000 | 12523833 | 12810291 |
| corrected / `39b70a103d99f422` | tiny-create-100 | host_sampler.sample_count | 3 | 34 | 33 | 34 |
| corrected / `39b70a103d99f422` | tiny-create-100 | host_sampler.sampled_peak_bytes | 3 | 48955392 | 48660480 | 49561600 |
| corrected / `39b70a103d99f422` | tiny-create-100 | inplace_edit_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | input.fixture_bytes | 3 | 524288000 | 524288000 | 524288000 |
| corrected / `39b70a103d99f422` | tiny-create-100 | input.regular_files | 3 | 100000 | 100000 | 100000 |
| corrected / `39b70a103d99f422` | tiny-create-100 | interrupted_syscall_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | lifecycle.anchor_prefetch_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | lifecycle.cleanup_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | lifecycle.docker_calls | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | lifecycle.docker_setup_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | lifecycle.helper_copy_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | lifecycle.mount_ready_ns | 3 | 9024875 | 7353792 | 11991292 |
| corrected / `39b70a103d99f422` | tiny-create-100 | lifecycle.proxy_ns | 3 | 167125 | 161875 | 211709 |
| corrected / `39b70a103d99f422` | tiny-create-100 | lifecycle.small_file_prefetch_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | lifecycle.small_file_prefetch_eligible | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | lifecycle.snapshot_cache_bytes_at_create | 3 | 1376 | 1376 | 1376 |
| corrected / `39b70a103d99f422` | tiny-create-100 | lifecycle.snapshot_cache_rows_at_create | 3 | 9 | 9 | 9 |
| corrected / `39b70a103d99f422` | tiny-create-100 | lifecycle.snapshot_database_bytes | 3 | 11684 | 10852 | 11876 |
| corrected / `39b70a103d99f422` | tiny-create-100 | lifecycle.snapshot_database_calls | 3 | 12 | 12 | 12 |
| corrected / `39b70a103d99f422` | tiny-create-100 | lifecycle.snapshot_database_rows | 3 | 12 | 12 | 12 |
| corrected / `39b70a103d99f422` | tiny-create-100 | lifecycle.snapshot_store_wide_scans | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | lifecycle.total_ns | 3 | 11470000 | 9432125 | 14236291 |
| corrected / `39b70a103d99f422` | tiny-create-100 | lifecycle.unattributed_ns | 3 | 211500 | 204584 | 248458 |
| corrected / `39b70a103d99f422` | tiny-create-100 | lifecycle.unmount_ns | 3 | 857875 | 795166 | 957041 |
| corrected / `39b70a103d99f422` | tiny-create-100 | lifecycle.wait_ns | 3 | 911458 | 827791 | 1213875 |
| corrected / `39b70a103d99f422` | tiny-create-100 | metadata_normalization_count | 3 | 110 | 110 | 110 |
| corrected / `39b70a103d99f422` | tiny-create-100 | metadata_normalization_ns | 3 | 76790458 | 70837042 | 76880584 |
| corrected / `39b70a103d99f422` | tiny-create-100 | orchestration_unattributed_ns | 3 | 30437375 | 30150333 | 31212833 |
| corrected / `39b70a103d99f422` | tiny-create-100 | preparation_ns | 3 | 1505435125 | 1500777042 | 1577498417 |
| corrected / `39b70a103d99f422` | tiny-create-100 | pure_call_sum_ns | 3 | 272562500 | 264093541 | 277999667 |
| corrected / `39b70a103d99f422` | tiny-create-100 | root_sync_ns | 3 | 742458 | 702625 | 791500 |
| corrected / `39b70a103d99f422` | tiny-create-100 | runtime_preparation_ns | 3 | 360887542 | 350359000 | 385958750 |
| corrected / `39b70a103d99f422` | tiny-create-100 | spool_boundary.max_allocated_bytes | 3 | 413696 | 413696 | 413696 |
| corrected / `39b70a103d99f422` | tiny-create-100 | spool_boundary.max_file_count | 3 | 101 | 101 | 101 |
| corrected / `39b70a103d99f422` | tiny-create-100 | spool_boundary.max_logical_bytes | 3 | 165033 | 165033 | 165033 |
| corrected / `39b70a103d99f422` | tiny-create-100 | store.allocated_bytes.delta | 3 | 16777216 | 16777216 | 16777216 |
| corrected / `39b70a103d99f422` | tiny-create-100 | store.allocated_bytes.end | 3 | 688979968 | 688914432 | 689373184 |
| corrected / `39b70a103d99f422` | tiny-create-100 | store.allocated_bytes.max | 3 | 688979968 | 688914432 | 689373184 |
| corrected / `39b70a103d99f422` | tiny-create-100 | store.allocated_bytes.start | 3 | 672202752 | 672137216 | 672595968 |
| corrected / `39b70a103d99f422` | tiny-create-100 | store.file_bytes.delta | 3 | 983040 | 917504 | 1048576 |
| corrected / `39b70a103d99f422` | tiny-create-100 | store.file_bytes.end | 3 | 673251328 | 673120256 | 673513472 |
| corrected / `39b70a103d99f422` | tiny-create-100 | store.file_bytes.max | 3 | 673251328 | 673120256 | 673513472 |
| corrected / `39b70a103d99f422` | tiny-create-100 | store.file_bytes.start | 3 | 672202752 | 672137216 | 672595968 |
| corrected / `39b70a103d99f422` | tiny-create-100 | store.freelist_page_count.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | store.freelist_page_count.end | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | store.freelist_page_count.max | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | store.freelist_page_count.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | store.live_page_bytes.delta | 3 | 983040 | 917504 | 1048576 |
| corrected / `39b70a103d99f422` | tiny-create-100 | store.live_page_bytes.end | 3 | 673251328 | 673120256 | 673513472 |
| corrected / `39b70a103d99f422` | tiny-create-100 | store.live_page_bytes.max | 3 | 673251328 | 673120256 | 673513472 |
| corrected / `39b70a103d99f422` | tiny-create-100 | store.live_page_bytes.start | 3 | 672202752 | 672137216 | 672595968 |
| corrected / `39b70a103d99f422` | tiny-create-100 | store.page_count.delta | 3 | 15 | 14 | 16 |
| corrected / `39b70a103d99f422` | tiny-create-100 | store.page_count.end | 3 | 10273 | 10271 | 10277 |
| corrected / `39b70a103d99f422` | tiny-create-100 | store.page_count.max | 3 | 10273 | 10271 | 10277 |
| corrected / `39b70a103d99f422` | tiny-create-100 | store.page_count.start | 3 | 10257 | 10256 | 10263 |
| corrected / `39b70a103d99f422` | tiny-create-100 | verified.canonical-verification.canonical_Chunk_bytes | 3 | 526726727 | 526725740 | 526726915 |
| corrected / `39b70a103d99f422` | tiny-create-100 | verified.canonical-verification.canonical_Chunk_objects | 3 | 108277 | 108230 | 108286 |
| corrected / `39b70a103d99f422` | tiny-create-100 | verified.canonical-verification.canonical_DirectoryNode_bytes | 3 | 4451473 | 4451473 | 4451473 |
| corrected / `39b70a103d99f422` | tiny-create-100 | verified.canonical-verification.canonical_DirectoryNode_objects | 3 | 1024 | 1024 | 1024 |
| corrected / `39b70a103d99f422` | tiny-create-100 | verified.canonical-verification.canonical_DirectoryState_bytes | 3 | 63112 | 63112 | 63112 |
| corrected / `39b70a103d99f422` | tiny-create-100 | verified.canonical-verification.canonical_DirectoryState_objects | 3 | 644 | 644 | 644 |
| corrected / `39b70a103d99f422` | tiny-create-100 | verified.canonical-verification.canonical_FileNode_bytes | 3 | 8735216 | 8733336 | 8735532 |
| corrected / `39b70a103d99f422` | tiny-create-100 | verified.canonical-verification.canonical_FileNode_objects | 3 | 100094 | 100093 | 100094 |
| corrected / `39b70a103d99f422` | tiny-create-100 | verified.canonical-verification.canonical_FileState_bytes | 3 | 10609964 | 10609858 | 10609964 |
| corrected / `39b70a103d99f422` | tiny-create-100 | verified.canonical-verification.canonical_FileState_objects | 3 | 100094 | 100093 | 100094 |
| corrected / `39b70a103d99f422` | tiny-create-100 | verified.canonical-verification.canonical_InodeRecord_bytes | 3 | 9872030 | 9871932 | 9872030 |
| corrected / `39b70a103d99f422` | tiny-create-100 | verified.canonical-verification.canonical_InodeRecord_objects | 3 | 100735 | 100734 | 100735 |
| corrected / `39b70a103d99f422` | tiny-create-100 | verified.canonical-verification.canonical_InodeTable_bytes | 3 | 6570780 | 6569916 | 6573048 |
| corrected / `39b70a103d99f422` | tiny-create-100 | verified.canonical-verification.canonical_InodeTable_objects | 3 | 1141 | 1133 | 1162 |
| corrected / `39b70a103d99f422` | tiny-create-100 | verified.canonical-verification.canonical_Metadata_bytes | 3 | 286 | 286 | 286 |
| corrected / `39b70a103d99f422` | tiny-create-100 | verified.canonical-verification.canonical_Metadata_objects | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | tiny-create-100 | verified.canonical-verification.canonical_Namespace_bytes | 3 | 121 | 121 | 121 |
| corrected / `39b70a103d99f422` | tiny-create-100 | verified.canonical-verification.canonical_Namespace_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-100 | verified.canonical-verification.canonical_unique_bytes | 3 | 567029110 | 567028845 | 567030009 |
| corrected / `39b70a103d99f422` | tiny-create-100 | verified.canonical-verification.canonical_unique_objects | 3 | 412004 | 411986 | 412018 |
| corrected / `39b70a103d99f422` | tiny-create-100 | verified.canonical-verification.independent_content_paths | 3 | 100100 | 100100 | 100100 |
| corrected / `39b70a103d99f422` | tiny-create-100 | verified.canonical-verification.logical_bytes | 3 | 524452890 | 524452890 | 524452890 |
| corrected / `39b70a103d99f422` | tiny-create-100 | verified.canonical-verification.persistence_custody_paths | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | verified.canonical-verification.verified_paths | 3 | 100744 | 100744 | 100744 |
| corrected / `39b70a103d99f422` | tiny-create-100 | verified.canonical-verification.verified_regular_paths | 3 | 100100 | 100100 | 100100 |
| corrected / `39b70a103d99f422` | tiny-create-100 | visibility_ns | 3 | 101042 | 93416 | 106625 |
| corrected / `39b70a103d99f422` | tiny-create-100 | visited_file_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | visited_path_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | workload_chmod_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | workload_close_call_count | 3 | 101 | 101 | 101 |
| corrected / `39b70a103d99f422` | tiny-create-100 | workload_closedir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | workload_fsync_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | workload_fsyncdir_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-100 | workload_ftruncate_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | workload_lstat_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | workload_mkdir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | workload_ns | 3 | 185947917 | 183819708 | 187254459 |
| corrected / `39b70a103d99f422` | tiny-create-100 | workload_open_call_count | 3 | 100 | 100 | 100 |
| corrected / `39b70a103d99f422` | tiny-create-100 | workload_open_directory_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-100 | workload_opendir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | workload_plan_ns | 3 | 848125 | 841208 | 920000 |
| corrected / `39b70a103d99f422` | tiny-create-100 | workload_pread_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | workload_pwrite_call_count | 3 | 90 | 90 | 90 |
| corrected / `39b70a103d99f422` | tiny-create-100 | workload_rename_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | workload_rmdir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | workload_symlink_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-100 | workload_unlink_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | attempted_syscall_count | 3 | 1453 | 1453 | 1453 |
| corrected / `39b70a103d99f422` | tiny-create-500 | benchmark_injection_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | benchmark_reopen_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | benchmark_verifier_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | cache_acquisition_ns | 3 | 603410875 | 599728375 | 605761750 |
| corrected / `39b70a103d99f422` | tiny-create-500 | cache_build_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | cache_validation_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | candidate.admission_transactions | 3 | 17 | 17 | 17 |
| corrected / `39b70a103d99f422` | tiny-create-500 | candidate.batch_inserted_bytes | 3 | 3003458 | 2973890 | 3003888 |
| corrected / `39b70a103d99f422` | tiny-create-500 | candidate.batch_inserted_objects | 3 | 2159 | 2159 | 2159 |
| corrected / `39b70a103d99f422` | tiny-create-500 | candidate.candidate_bytes | 3 | 3551312 | 3444226 | 3551492 |
| corrected / `39b70a103d99f422` | tiny-create-500 | candidate.candidate_objects | 3 | 2259 | 2246 | 2266 |
| corrected / `39b70a103d99f422` | tiny-create-500 | candidate.final_inserted_bytes | 3 | 547273 | 439757 | 577021 |
| corrected / `39b70a103d99f422` | tiny-create-500 | candidate.final_inserted_objects | 3 | 93 | 80 | 100 |
| corrected / `39b70a103d99f422` | tiny-create-500 | candidate.inserted_bytes | 3 | 3550731 | 3443645 | 3550911 |
| corrected / `39b70a103d99f422` | tiny-create-500 | candidate.inserted_objects | 3 | 2252 | 2239 | 2259 |
| corrected / `39b70a103d99f422` | tiny-create-500 | candidate.max_transaction_bytes | 3 | 547273 | 528374 | 686418 |
| corrected / `39b70a103d99f422` | tiny-create-500 | candidate.max_transaction_objects | 3 | 127 | 127 | 127 |
| corrected / `39b70a103d99f422` | tiny-create-500 | candidate.preexisting_reused_bytes | 3 | 581 | 581 | 581 |
| corrected / `39b70a103d99f422` | tiny-create-500 | candidate.preexisting_reused_objects | 3 | 7 | 7 | 7 |
| corrected / `39b70a103d99f422` | tiny-create-500 | candidate.reused_bytes | 3 | 581 | 581 | 581 |
| corrected / `39b70a103d99f422` | tiny-create-500 | candidate.reused_objects | 3 | 7 | 7 | 7 |
| corrected / `39b70a103d99f422` | tiny-create-500 | cgroup.cpu_usage_usec_delta | 3 | 187906 | 185623 | 187945 |
| corrected / `39b70a103d99f422` | tiny-create-500 | cgroup.cpu_usage_usec_end | 3 | 230528 | 228695 | 231632 |
| corrected / `39b70a103d99f422` | tiny-create-500 | cgroup.cpu_usage_usec_start | 3 | 43072 | 42622 | 43687 |
| corrected / `39b70a103d99f422` | tiny-create-500 | cgroup.observed_max.memory.current | 3 | 5066752 | 4800512 | 5120000 |
| corrected / `39b70a103d99f422` | tiny-create-500 | cgroup.observed_max.memory.events.oom | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | cgroup.observed_max.memory.events.oom_kill | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | cgroup.observed_max.memory.peak | 3 | 5378048 | 5140480 | 5554176 |
| corrected / `39b70a103d99f422` | tiny-create-500 | cgroup.observed_max.memory.stat.anon | 3 | 2387968 | 2387968 | 2392064 |
| corrected / `39b70a103d99f422` | tiny-create-500 | cgroup.observed_max.memory.stat.file | 3 | 4096 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | tiny-create-500 | cgroup.observed_max.memory.stat.file_dirty | 3 | 4096 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | tiny-create-500 | cgroup.observed_max.memory.stat.file_writeback | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | cgroup.observed_max.memory.stat.kernel | 3 | 1884160 | 1884160 | 1888256 |
| corrected / `39b70a103d99f422` | tiny-create-500 | cgroup.observed_max.memory.stat.shmem | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | cgroup.observed_max.memory.stat.slab | 3 | 1251216 | 1251168 | 1252120 |
| corrected / `39b70a103d99f422` | tiny-create-500 | cgroup.observed_max.memory.swap.current | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | cgroup.observed_max.pids.current | 3 | 17 | 17 | 17 |
| corrected / `39b70a103d99f422` | tiny-create-500 | cleanup_ns | 3 | 391615125 | 357693708 | 393055917 |
| corrected / `39b70a103d99f422` | tiny-create-500 | clone_bytes | 3 | 672202752 | 672137216 | 672595968 |
| corrected / `39b70a103d99f422` | tiny-create-500 | clone_wall_ns | 3 | 471734792 | 461300792 | 490852292 |
| corrected / `39b70a103d99f422` | tiny-create-500 | command_wall_ns | 3 | 3100862917 | 3097633333 | 3170072583 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_diagnostics.cdc_bytes_scanned | 3 | 824450 | 824450 | 824450 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_diagnostics.edit_count | 3 | 450 | 450 | 450 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_diagnostics.edit_metric_nodes_scanned | 3 | 450 | 450 | 450 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_diagnostics.edit_piece_count | 3 | 450 | 450 | 450 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_diagnostics.edit_piece_height | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_diagnostics.edit_piece_logical_charge | 3 | 3600 | 3600 | 3600 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_diagnostics.edit_spool_allocated_bytes | 3 | 824450 | 824450 | 824450 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_diagnostics.edit_spool_live_bytes | 3 | 824450 | 824450 | 824450 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_diagnostics.edit_spool_peak_bytes | 3 | 824450 | 824450 | 824450 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_diagnostics.edit_spool_superseded_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_diagnostics.edit_tree_visits | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_diagnostics.namespace_base_paths_visited | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_diagnostics.namespace_candidate_probe_nodes | 3 | 540 | 523 | 637 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_diagnostics.namespace_clean_nodes_visited | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_diagnostics.namespace_dirty_nodes_visited | 3 | 510 | 510 | 510 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_diagnostics.namespace_final_paths_visited | 3 | 3500 | 3500 | 3500 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_diagnostics.physical_spool_allocated_bytes | 3 | 2048000 | 2048000 | 2048000 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_diagnostics.physical_spool_observation_count | 3 | 2350 | 2350 | 2350 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_diagnostics.physical_spool_observation_errors | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_diagnostics.physical_spool_peak_bytes | 3 | 2048000 | 2048000 | 2048000 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_ns | 3 | 333305833 | 316954875 | 343716375 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.candidate_finish_ns | 3 | 14336250 | 14101542 | 14574167 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.candidate_plan_ns | 3 | 625 | 583 | 792 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.capture_ns | 3 | 7083 | 6917 | 9458 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.captured_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.captured_files | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.content_ns | 3 | 63208500 | 61534917 | 63231208 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.dirty_compare_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.in_place_rebase_ns | 3 | 123989792 | 115684208 | 130656041 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.local_admission_ns | 3 | 5917917 | 5746000 | 6200417 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.max_admission_transaction_bytes | 3 | 547273 | 528374 | 686418 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.max_admission_transaction_objects | 3 | 127 | 127 | 127 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.namespace_ns | 3 | 59588875 | 59563625 | 59681083 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.object_admission_begin_ns | 3 | 122081 | 116126 | 129832 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.object_admission_commit_ns | 3 | 31212956 | 29154584 | 46322418 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.object_admission_insert_ns | 3 | 23788087 | 22999425 | 24470882 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.object_admission_ns | 3 | 55579500 | 55182458 | 71593250 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.object_admission_transactions | 3 | 17 | 17 | 17 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.pause_fence_ns | 3 | 247625 | 204792 | 346333 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.payload_bytes_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.publication_begin_ns | 3 | 6000 | 5667 | 7750 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.publication_commit_ns | 3 | 1148917 | 1014417 | 1195709 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.publication_insert_ns | 3 | 1164419 | 1003376 | 1169212 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.publication_metadata_ns | 3 | 117792 | 117291 | 123334 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.publication_ns | 3 | 2446625 | 2149584 | 2505209 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.publication_payload_ns | 3 | 2128 | 1996 | 2541 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.quiesce_ns | 3 | 334 | 333 | 375 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.resume_ns | 3 | 847166 | 354708 | 1350875 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.snapshot_database_bytes | 3 | 12277535 | 12014891 | 12277927 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.snapshot_database_calls | 3 | 3829 | 3805 | 3839 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.snapshot_database_rows | 3 | 3829 | 3805 | 3839 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.total_ns | 3 | 333301500 | 316950500 | 343712209 |
| corrected / `39b70a103d99f422` | tiny-create-500 | commit_work.unattributed_ns | 3 | 1067416 | 952668 | 1094958 |
| corrected / `39b70a103d99f422` | tiny-create-500 | completed_chain_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | completed_episode_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | completed_file_write_count | 3 | 500 | 500 | 500 |
| corrected / `39b70a103d99f422` | tiny-create-500 | completed_read_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | completed_read_request_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | completed_syscall_count | 3 | 1453 | 1453 | 1453 |
| corrected / `39b70a103d99f422` | tiny-create-500 | completed_target_count | 3 | 500 | 500 | 500 |
| corrected / `39b70a103d99f422` | tiny-create-500 | completed_write_bytes | 3 | 824450 | 824450 | 824450 |
| corrected / `39b70a103d99f422` | tiny-create-500 | create_ns | 3 | 9637750 | 8889667 | 10608417 |
| corrected / `39b70a103d99f422` | tiny-create-500 | created_commit_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-500 | directory_entry_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | editor_save_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | end_ns | 3 | 4860083 | 3962792 | 8694125 |
| corrected / `39b70a103d99f422` | tiny-create-500 | exec_ns | 3 | 735669916 | 730030500 | 808178625 |
| corrected / `39b70a103d99f422` | tiny-create-500 | external_process_wall_ns | 3 | 1214255250 | 1202526333 | 1256806625 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.callback_access | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.callback_create | 3 | 500 | 500 | 500 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.callback_flush | 3 | 500 | 500 | 500 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.callback_fsync | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.callback_fsyncdir | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.callback_getattr | 3 | 501 | 501 | 501 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.callback_link | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.callback_lookup | 3 | 521 | 521 | 521 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.callback_mkdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.callback_mknod | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.callback_open | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.callback_opendir | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.callback_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.callback_readdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.callback_readdirplus | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.callback_readlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.callback_release | 3 | 500 | 500 | 500 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.callback_releasedir | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.callback_rename | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.callback_rmdir | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.callback_setattr | 3 | 1020 | 1020 | 1020 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.callback_statfs | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.callback_symlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.callback_unlink | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.callback_write | 3 | 450 | 450 | 450 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.client_decode_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.client_decode_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.client_response_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.client_response_frames | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.client_socket_read_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.collection_ns | 3 | 712042 | 532334 | 1263291 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.directory_entries_returned | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.directory_nonzero_offset_requests | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.host_dispatch_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.host_encode_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.host_response_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.host_response_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.host_response_frames | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.host_socket_write_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.init_capabilities | 3 | 4481057 | 4481057 | 4481057 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.kernel_read_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.kernel_read_gt_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.kernel_read_le_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.kernel_read_le_256k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.kernel_read_le_4k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.kernel_read_le_64k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.kernel_read_requests | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.local_bytes | 3 | 13576217 | 13314398 | 13582625 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.local_calls | 3 | 14897 | 14876 | 14907 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.local_ids | 3 | 14897 | 14876 | 14907 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.local_read_auth_ns | 3 | 95486569 | 94701155 | 97817435 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.local_rows | 3 | 14897 | 14876 | 14907 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.max_payload_batch | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.max_readahead_bytes | 3 | 131072 | 131072 | 131072 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.payload_batches | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.payload_bytes_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.payload_ids | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.read_ahead_cache_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.read_ahead_fetched_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.read_ahead_fetches | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.read_ahead_hits | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.read_ahead_misses | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.read_ahead_requested_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.read_ahead_served_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.read_ahead_unused_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.read_plan_builds | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.rope_nodes_read | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.snapshot_cache_bytes | 3 | 1172982 | 1172982 | 1173231 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.snapshot_cache_hits | 3 | 11027 | 11027 | 11030 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.snapshot_cache_rows | 3 | 11027 | 11027 | 11030 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.snapshot_database_bytes | 3 | 12403235 | 12141167 | 12409643 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.snapshot_database_calls | 3 | 3870 | 3846 | 3880 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.snapshot_database_rows | 3 | 3870 | 3846 | 3880 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.workspace_output_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.workspace_read_calls | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.workspace_read_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_read.workspace_requested_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_write.client_frame_bytes | 3 | 835700 | 835700 | 835700 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_write.client_request_copy_bytes | 3 | 824450 | 824450 | 824450 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_write.collection_ns | 3 | 421000 | 347792 | 427458 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_write.decode_ns | 3 | 364364 | 353882 | 441923 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_write.encode_ns | 3 | 19044 | 18380 | 19166 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_write.frame_payload_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_write.host_decode_copy_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_write.host_dispatch_ns | 3 | 20868506 | 19654869 | 31060333 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_write.host_frame_bytes | 3 | 835700 | 835700 | 835700 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_write.kernel_write_bytes | 3 | 824450 | 824450 | 824450 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_write.kernel_write_gt_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_write.kernel_write_le_1m | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_write.kernel_write_le_256k | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_write.kernel_write_le_4k | 3 | 400 | 400 | 400 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_write.kernel_write_le_64k | 3 | 50 | 50 | 50 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_write.kernel_write_requests | 3 | 450 | 450 | 450 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_write.max_write_bytes | 3 | 1048576 | 1048576 | 1048576 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_write.socket_read_ns | 3 | 199945365 | 191447201 | 202247425 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_write.socket_write_ns | 3 | 8241505 | 8106546 | 8571235 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_write.spool_write_bytes | 3 | 824450 | 824450 | 824450 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_write.spool_write_ns | 3 | 67794798 | 65247247 | 75778036 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_write.spool_write_open_count | 3 | 500 | 500 | 500 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_write.workspace_fence_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-500 | fuse_write.workspace_fence_ns | 3 | 2009083 | 2004500 | 2032542 |
| corrected / `39b70a103d99f422` | tiny-create-500 | git_process_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | host.disk_read_bytes.delta | 3 | 45056 | 8192 | 237568 |
| corrected / `39b70a103d99f422` | tiny-create-500 | host.disk_read_bytes.end | 3 | 45056 | 8192 | 237568 |
| corrected / `39b70a103d99f422` | tiny-create-500 | host.disk_read_bytes.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | host.disk_write_bytes.delta | 3 | 2060288 | 8192 | 2060288 |
| corrected / `39b70a103d99f422` | tiny-create-500 | host.disk_write_bytes.end | 3 | 2060288 | 8192 | 2060288 |
| corrected / `39b70a103d99f422` | tiny-create-500 | host.disk_write_bytes.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | host.peak_resident_bytes.max | 3 | 72826880 | 72122368 | 73056256 |
| corrected / `39b70a103d99f422` | tiny-create-500 | host.physical_footprint_bytes.max | 3 | 32293392 | 31687208 | 32309800 |
| corrected / `39b70a103d99f422` | tiny-create-500 | host.resident_bytes.max | 3 | 72777728 | 72073216 | 73007104 |
| corrected / `39b70a103d99f422` | tiny-create-500 | host.swaps.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | host.swaps.end | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | host.swaps.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | host.system_cpu_ns.delta | 3 | 256238125 | 234022833 | 256895166 |
| corrected / `39b70a103d99f422` | tiny-create-500 | host.system_cpu_ns.end | 3 | 257807833 | 236051583 | 258609541 |
| corrected / `39b70a103d99f422` | tiny-create-500 | host.system_cpu_ns.start | 3 | 1714375 | 1569708 | 2028750 |
| corrected / `39b70a103d99f422` | tiny-create-500 | host.user_cpu_ns.delta | 3 | 247082791 | 246253083 | 247150292 |
| corrected / `39b70a103d99f422` | tiny-create-500 | host.user_cpu_ns.end | 3 | 248899916 | 248208791 | 249012250 |
| corrected / `39b70a103d99f422` | tiny-create-500 | host.user_cpu_ns.start | 3 | 1861958 | 1817125 | 1955708 |
| corrected / `39b70a103d99f422` | tiny-create-500 | host_orchestration_ns | 3 | 1125307000 | 1091580917 | 1193099250 |
| corrected / `39b70a103d99f422` | tiny-create-500 | host_sampler.baseline_bytes | 3 | 2588672 | 2588672 | 2605056 |
| corrected / `39b70a103d99f422` | tiny-create-500 | host_sampler.final_bytes | 3 | 35389440 | 34783232 | 35405824 |
| corrected / `39b70a103d99f422` | tiny-create-500 | host_sampler.maximum_gap_ns | 3 | 12539375 | 12537792 | 12559416 |
| corrected / `39b70a103d99f422` | tiny-create-500 | host_sampler.sample_count | 3 | 108 | 105 | 113 |
| corrected / `39b70a103d99f422` | tiny-create-500 | host_sampler.sampled_peak_bytes | 3 | 72810496 | 72105984 | 73023488 |
| corrected / `39b70a103d99f422` | tiny-create-500 | inplace_edit_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | input.fixture_bytes | 3 | 524288000 | 524288000 | 524288000 |
| corrected / `39b70a103d99f422` | tiny-create-500 | input.regular_files | 3 | 100000 | 100000 | 100000 |
| corrected / `39b70a103d99f422` | tiny-create-500 | interrupted_syscall_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | lifecycle.anchor_prefetch_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | lifecycle.cleanup_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | lifecycle.docker_calls | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | lifecycle.docker_setup_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | lifecycle.helper_copy_ns | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | lifecycle.mount_ready_ns | 3 | 8136250 | 7522750 | 9242250 |
| corrected / `39b70a103d99f422` | tiny-create-500 | lifecycle.proxy_ns | 3 | 172125 | 161542 | 199000 |
| corrected / `39b70a103d99f422` | tiny-create-500 | lifecycle.small_file_prefetch_bytes | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | lifecycle.small_file_prefetch_eligible | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | lifecycle.snapshot_cache_bytes_at_create | 3 | 1376 | 1376 | 1376 |
| corrected / `39b70a103d99f422` | tiny-create-500 | lifecycle.snapshot_cache_rows_at_create | 3 | 9 | 9 | 9 |
| corrected / `39b70a103d99f422` | tiny-create-500 | lifecycle.snapshot_database_bytes | 3 | 11684 | 10852 | 11876 |
| corrected / `39b70a103d99f422` | tiny-create-500 | lifecycle.snapshot_database_calls | 3 | 12 | 12 | 12 |
| corrected / `39b70a103d99f422` | tiny-create-500 | lifecycle.snapshot_database_rows | 3 | 12 | 12 | 12 |
| corrected / `39b70a103d99f422` | tiny-create-500 | lifecycle.snapshot_store_wide_scans | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | lifecycle.total_ns | 3 | 11328249 | 10079208 | 15081750 |
| corrected / `39b70a103d99f422` | tiny-create-500 | lifecycle.unattributed_ns | 3 | 213790 | 207583 | 227208 |
| corrected / `39b70a103d99f422` | tiny-create-500 | lifecycle.unmount_ns | 3 | 1462000 | 1234167 | 3422500 |
| corrected / `39b70a103d99f422` | tiny-create-500 | lifecycle.wait_ns | 3 | 1317209 | 933541 | 2037292 |
| corrected / `39b70a103d99f422` | tiny-create-500 | metadata_normalization_count | 3 | 510 | 510 | 510 |
| corrected / `39b70a103d99f422` | tiny-create-500 | metadata_normalization_ns | 3 | 255043459 | 242462251 | 321465917 |
| corrected / `39b70a103d99f422` | tiny-create-500 | orchestration_unattributed_ns | 3 | 31634041 | 31319084 | 32140292 |
| corrected / `39b70a103d99f422` | tiny-create-500 | preparation_ns | 3 | 1520823208 | 1492616875 | 1536104666 |
| corrected / `39b70a103d99f422` | tiny-create-500 | pure_call_sum_ns | 3 | 1093987916 | 1059946876 | 1160958958 |
| corrected / `39b70a103d99f422` | tiny-create-500 | root_sync_ns | 3 | 2399541 | 2344125 | 2433375 |
| corrected / `39b70a103d99f422` | tiny-create-500 | runtime_preparation_ns | 3 | 361555750 | 343703875 | 367694000 |
| corrected / `39b70a103d99f422` | tiny-create-500 | spool_boundary.max_allocated_bytes | 3 | 2052096 | 2052096 | 2052096 |
| corrected / `39b70a103d99f422` | tiny-create-500 | spool_boundary.max_file_count | 3 | 501 | 501 | 501 |
| corrected / `39b70a103d99f422` | tiny-create-500 | spool_boundary.max_logical_bytes | 3 | 824593 | 824593 | 824593 |
| corrected / `39b70a103d99f422` | tiny-create-500 | store.allocated_bytes.delta | 3 | 16777216 | 16777216 | 16777216 |
| corrected / `39b70a103d99f422` | tiny-create-500 | store.allocated_bytes.end | 3 | 688979968 | 688914432 | 689373184 |
| corrected / `39b70a103d99f422` | tiny-create-500 | store.allocated_bytes.max | 3 | 688979968 | 688914432 | 689373184 |
| corrected / `39b70a103d99f422` | tiny-create-500 | store.allocated_bytes.start | 3 | 672202752 | 672137216 | 672595968 |
| corrected / `39b70a103d99f422` | tiny-create-500 | store.file_bytes.delta | 3 | 3866624 | 3801088 | 3997696 |
| corrected / `39b70a103d99f422` | tiny-create-500 | store.file_bytes.end | 3 | 676200448 | 675938304 | 676462592 |
| corrected / `39b70a103d99f422` | tiny-create-500 | store.file_bytes.max | 3 | 676200448 | 675938304 | 676462592 |
| corrected / `39b70a103d99f422` | tiny-create-500 | store.file_bytes.start | 3 | 672202752 | 672137216 | 672595968 |
| corrected / `39b70a103d99f422` | tiny-create-500 | store.freelist_page_count.delta | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | store.freelist_page_count.end | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | store.freelist_page_count.max | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | store.freelist_page_count.start | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | store.live_page_bytes.delta | 3 | 3866624 | 3801088 | 3997696 |
| corrected / `39b70a103d99f422` | tiny-create-500 | store.live_page_bytes.end | 3 | 676200448 | 675938304 | 676462592 |
| corrected / `39b70a103d99f422` | tiny-create-500 | store.live_page_bytes.max | 3 | 676200448 | 675938304 | 676462592 |
| corrected / `39b70a103d99f422` | tiny-create-500 | store.live_page_bytes.start | 3 | 672202752 | 672137216 | 672595968 |
| corrected / `39b70a103d99f422` | tiny-create-500 | store.page_count.delta | 3 | 59 | 58 | 61 |
| corrected / `39b70a103d99f422` | tiny-create-500 | store.page_count.end | 3 | 10318 | 10314 | 10322 |
| corrected / `39b70a103d99f422` | tiny-create-500 | store.page_count.max | 3 | 10318 | 10314 | 10322 |
| corrected / `39b70a103d99f422` | tiny-create-500 | store.page_count.start | 3 | 10257 | 10256 | 10263 |
| corrected / `39b70a103d99f422` | tiny-create-500 | verified.canonical-verification.canonical_Chunk_bytes | 3 | 527393781 | 527392728 | 527393991 |
| corrected / `39b70a103d99f422` | tiny-create-500 | verified.canonical-verification.canonical_Chunk_objects | 3 | 108634 | 108584 | 108644 |
| corrected / `39b70a103d99f422` | tiny-create-500 | verified.canonical-verification.canonical_DirectoryNode_bytes | 3 | 4468273 | 4468273 | 4468273 |
| corrected / `39b70a103d99f422` | tiny-create-500 | verified.canonical-verification.canonical_DirectoryNode_objects | 3 | 1024 | 1024 | 1024 |
| corrected / `39b70a103d99f422` | tiny-create-500 | verified.canonical-verification.canonical_DirectoryState_bytes | 3 | 63112 | 63112 | 63112 |
| corrected / `39b70a103d99f422` | tiny-create-500 | verified.canonical-verification.canonical_DirectoryState_objects | 3 | 644 | 644 | 644 |
| corrected / `39b70a103d99f422` | tiny-create-500 | verified.canonical-verification.canonical_FileNode_bytes | 3 | 8765204 | 8763072 | 8765604 |
| corrected / `39b70a103d99f422` | tiny-create-500 | verified.canonical-verification.canonical_FileNode_objects | 3 | 100451 | 100448 | 100451 |
| corrected / `39b70a103d99f422` | tiny-create-500 | verified.canonical-verification.canonical_FileState_bytes | 3 | 10647806 | 10647488 | 10647806 |
| corrected / `39b70a103d99f422` | tiny-create-500 | verified.canonical-verification.canonical_FileState_objects | 3 | 100451 | 100448 | 100451 |
| corrected / `39b70a103d99f422` | tiny-create-500 | verified.canonical-verification.canonical_InodeRecord_bytes | 3 | 9907016 | 9906722 | 9907016 |
| corrected / `39b70a103d99f422` | tiny-create-500 | verified.canonical-verification.canonical_InodeRecord_objects | 3 | 101092 | 101089 | 101092 |
| corrected / `39b70a103d99f422` | tiny-create-500 | verified.canonical-verification.canonical_InodeTable_bytes | 3 | 6596920 | 6595840 | 6599296 |
| corrected / `39b70a103d99f422` | tiny-create-500 | verified.canonical-verification.canonical_InodeTable_objects | 3 | 1146 | 1136 | 1168 |
| corrected / `39b70a103d99f422` | tiny-create-500 | verified.canonical-verification.canonical_Metadata_bytes | 3 | 286 | 286 | 286 |
| corrected / `39b70a103d99f422` | tiny-create-500 | verified.canonical-verification.canonical_Metadata_objects | 3 | 2 | 2 | 2 |
| corrected / `39b70a103d99f422` | tiny-create-500 | verified.canonical-verification.canonical_Namespace_bytes | 3 | 121 | 121 | 121 |
| corrected / `39b70a103d99f422` | tiny-create-500 | verified.canonical-verification.canonical_Namespace_objects | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-500 | verified.canonical-verification.canonical_unique_bytes | 3 | 567841439 | 567841098 | 567843129 |
| corrected / `39b70a103d99f422` | tiny-create-500 | verified.canonical-verification.canonical_unique_objects | 3 | 413435 | 413408 | 413455 |
| corrected / `39b70a103d99f422` | tiny-create-500 | verified.canonical-verification.independent_content_paths | 3 | 100500 | 100500 | 100500 |
| corrected / `39b70a103d99f422` | tiny-create-500 | verified.canonical-verification.logical_bytes | 3 | 525112450 | 525112450 | 525112450 |
| corrected / `39b70a103d99f422` | tiny-create-500 | verified.canonical-verification.persistence_custody_paths | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | verified.canonical-verification.verified_paths | 3 | 101144 | 101144 | 101144 |
| corrected / `39b70a103d99f422` | tiny-create-500 | verified.canonical-verification.verified_regular_paths | 3 | 100500 | 100500 | 100500 |
| corrected / `39b70a103d99f422` | tiny-create-500 | visibility_ns | 3 | 109042 | 103792 | 171958 |
| corrected / `39b70a103d99f422` | tiny-create-500 | visited_file_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | visited_path_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | workload_chmod_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | workload_close_call_count | 3 | 501 | 501 | 501 |
| corrected / `39b70a103d99f422` | tiny-create-500 | workload_closedir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | workload_fsync_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | workload_fsyncdir_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-500 | workload_ftruncate_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | workload_lstat_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | workload_mkdir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | workload_ns | 3 | 730783084 | 725546875 | 802790959 |
| corrected / `39b70a103d99f422` | tiny-create-500 | workload_open_call_count | 3 | 500 | 500 | 500 |
| corrected / `39b70a103d99f422` | tiny-create-500 | workload_open_directory_call_count | 3 | 1 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-create-500 | workload_opendir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | workload_plan_ns | 3 | 799625 | 797625 | 814959 |
| corrected / `39b70a103d99f422` | tiny-create-500 | workload_pread_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | workload_pwrite_call_count | 3 | 450 | 450 | 450 |
| corrected / `39b70a103d99f422` | tiny-create-500 | workload_rename_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | workload_rmdir_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | workload_symlink_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-create-500 | workload_unlink_call_count | 3 | 0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | attempted_syscall_count | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | benchmark_injection_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | benchmark_reopen_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | benchmark_verifier_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | cache_acquisition_ns | 2 | 500491250.0 | 492026083 | 508956417 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | cache_build_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | cache_validation_ns | 2 | 497356458.0 | 489183791 | 505529125 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | cgroup.cpu_usage_usec_delta | 2 | 9127.0 | 8084 | 10170 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | cgroup.cpu_usage_usec_end | 2 | 58579.0 | 57470 | 59688 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | cgroup.cpu_usage_usec_start | 2 | 49452.0 | 49386 | 49518 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | cgroup.observed_max.memory.current | 2 | 3342336.0 | 2981888 | 3702784 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | cgroup.observed_max.memory.events.oom | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | cgroup.observed_max.memory.events.oom_kill | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | cgroup.observed_max.memory.peak | 2 | 4263936.0 | 4231168 | 4296704 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | cgroup.observed_max.memory.stat.anon | 2 | 1083392.0 | 1060864 | 1105920 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | cgroup.observed_max.memory.stat.file | 2 | 4096.0 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | cgroup.observed_max.memory.stat.file_dirty | 2 | 4096.0 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | cgroup.observed_max.memory.stat.file_writeback | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | cgroup.observed_max.memory.stat.kernel | 2 | 1325056.0 | 1306624 | 1343488 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | cgroup.observed_max.memory.stat.shmem | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | cgroup.observed_max.memory.stat.slab | 2 | 664840.0 | 660024 | 669656 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | cgroup.observed_max.memory.swap.current | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | cgroup.observed_max.pids.current | 2 | 17.0 | 17 | 17 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | cleanup_ns | 2 | 306631187.0 | 290398916 | 322863458 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | clone_bytes | 2 | 673447936.0 | 673251328 | 673644544 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | clone_wall_ns | 2 | 472328583.0 | 468063708 | 476593458 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | command_wall_ns | 2 | 4927016667.0 | 4844716042 | 5009317292 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_diagnostics.cdc_bytes_scanned | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_diagnostics.edit_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_diagnostics.edit_metric_nodes_scanned | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_diagnostics.edit_piece_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_diagnostics.edit_piece_height | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_diagnostics.edit_piece_logical_charge | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_diagnostics.edit_spool_allocated_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_diagnostics.edit_spool_live_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_diagnostics.edit_spool_peak_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_diagnostics.edit_spool_superseded_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_diagnostics.edit_tree_visits | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_diagnostics.namespace_base_paths_visited | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_diagnostics.namespace_candidate_probe_nodes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_diagnostics.namespace_clean_nodes_visited | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_diagnostics.namespace_dirty_nodes_visited | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_diagnostics.namespace_final_paths_visited | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_diagnostics.physical_spool_allocated_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_diagnostics.physical_spool_observation_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_diagnostics.physical_spool_observation_errors | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_diagnostics.physical_spool_peak_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_ns | 2 | 1730000.5 | 956042 | 2503959 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.candidate_finish_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.candidate_plan_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.capture_ns | 2 | 6791.5 | 6625 | 6958 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.captured_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.captured_files | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.content_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.dirty_compare_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.in_place_rebase_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.local_admission_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.max_admission_transaction_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.max_admission_transaction_objects | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.namespace_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.object_admission_begin_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.object_admission_commit_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.object_admission_insert_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.object_admission_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.object_admission_transactions | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.pause_fence_ns | 2 | 366791.5 | 333542 | 400041 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.payload_bytes_read | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.publication_begin_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.publication_commit_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.publication_insert_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.publication_metadata_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.publication_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.publication_payload_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.quiesce_ns | 2 | 250.5 | 209 | 292 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.resume_ns | 2 | 767979.0 | 241958 | 1294000 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.snapshot_database_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.snapshot_database_calls | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.snapshot_database_rows | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.total_ns | 2 | 1726291.5 | 952791 | 2499792 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | commit_work.unattributed_ns | 2 | 584479.0 | 370457 | 798501 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | completed_chain_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | completed_episode_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | completed_file_write_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | completed_read_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | completed_read_request_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | completed_syscall_count | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | completed_target_count | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | completed_write_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | create_ns | 2 | 13490979.5 | 10108875 | 16873084 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | created_commit_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | directory_entry_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | editor_save_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | end_ns | 2 | 5885729.0 | 3714708 | 8056750 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | exec_ns | 2 | 7414812.5 | 6369708 | 8459917 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | external_process_wall_ns | 2 | 110313771.0 | 107877208 | 112750334 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.callback_access | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.callback_create | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.callback_flush | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.callback_fsync | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.callback_fsyncdir | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.callback_getattr | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.callback_link | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.callback_lookup | 2 | 3.0 | 3 | 3 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.callback_mkdir | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.callback_mknod | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.callback_open | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.callback_opendir | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.callback_read | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.callback_readdir | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.callback_readdirplus | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.callback_readlink | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.callback_release | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.callback_releasedir | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.callback_rename | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.callback_rmdir | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.callback_setattr | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.callback_statfs | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.callback_symlink | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.callback_unlink | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.callback_write | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.client_decode_copy_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.client_decode_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.client_response_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.client_response_frames | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.client_socket_read_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.collection_ns | 2 | 558937.5 | 499667 | 618208 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.directory_entries_returned | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.directory_nonzero_offset_requests | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.host_dispatch_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.host_encode_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.host_response_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.host_response_copy_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.host_response_frames | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.host_socket_write_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.init_capabilities | 2 | 4481057.0 | 4481057 | 4481057 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.kernel_read_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.kernel_read_gt_1m | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.kernel_read_le_1m | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.kernel_read_le_256k | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.kernel_read_le_4k | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.kernel_read_le_64k | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.kernel_read_requests | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.local_bytes | 2 | 38496.0 | 37344 | 39648 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.local_calls | 2 | 43.0 | 43 | 43 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.local_ids | 2 | 43.0 | 43 | 43 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.local_read_auth_ns | 2 | 1279395.0 | 1030416 | 1528374 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.local_rows | 2 | 43.0 | 43 | 43 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.max_payload_batch | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.max_readahead_bytes | 2 | 131072.0 | 131072 | 131072 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.payload_batches | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.payload_bytes_read | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.payload_ids | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.read_ahead_cache_copy_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.read_ahead_fetched_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.read_ahead_fetches | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.read_ahead_hits | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.read_ahead_misses | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.read_ahead_requested_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.read_ahead_served_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.read_ahead_unused_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.read_plan_builds | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.rope_nodes_read | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.snapshot_cache_bytes | 2 | 3224.0 | 1814 | 4634 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.snapshot_cache_hits | 2 | 21.5 | 20 | 23 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.snapshot_cache_rows | 2 | 21.5 | 20 | 23 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.snapshot_database_bytes | 2 | 35272.0 | 35014 | 35530 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.snapshot_database_calls | 2 | 21.5 | 20 | 23 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.snapshot_database_rows | 2 | 21.5 | 20 | 23 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.workspace_output_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.workspace_read_calls | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.workspace_read_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_read.workspace_requested_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_write.client_frame_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_write.client_request_copy_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_write.collection_ns | 2 | 467458.5 | 325125 | 609792 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_write.decode_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_write.encode_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_write.frame_payload_copy_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_write.host_decode_copy_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_write.host_dispatch_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_write.host_frame_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_write.kernel_write_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_write.kernel_write_gt_1m | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_write.kernel_write_le_1m | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_write.kernel_write_le_256k | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_write.kernel_write_le_4k | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_write.kernel_write_le_64k | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_write.kernel_write_requests | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_write.max_write_bytes | 2 | 1048576.0 | 1048576 | 1048576 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_write.socket_read_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_write.socket_write_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_write.spool_write_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_write.spool_write_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_write.spool_write_open_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_write.workspace_fence_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | fuse_write.workspace_fence_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | git_process_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | host.disk_read_bytes.delta | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | host.disk_read_bytes.end | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | host.disk_read_bytes.start | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | host.disk_write_bytes.delta | 2 | 10240.0 | 8192 | 12288 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | host.disk_write_bytes.end | 2 | 10240.0 | 8192 | 12288 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | host.disk_write_bytes.start | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | host.peak_resident_bytes.max | 2 | 10690560.0 | 10567680 | 10813440 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | host.physical_footprint_bytes.max | 2 | 3514800.0 | 3473840 | 3555760 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | host.resident_bytes.max | 2 | 10690560.0 | 10567680 | 10813440 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | host.swaps.delta | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | host.swaps.end | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | host.swaps.start | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | host.system_cpu_ns.delta | 2 | 29396229.0 | 27610625 | 31181833 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | host.system_cpu_ns.end | 2 | 31074229.0 | 29205000 | 32943458 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | host.system_cpu_ns.start | 2 | 1678000.0 | 1594375 | 1761625 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | host.user_cpu_ns.delta | 2 | 11472979.0 | 11000583 | 11945375 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | host.user_cpu_ns.end | 2 | 13320645.5 | 12826708 | 13814583 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | host.user_cpu_ns.start | 2 | 1847666.5 | 1826125 | 1869208 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | host_orchestration_ns | 2 | 60753833.5 | 55192875 | 66314792 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | host_sampler.baseline_bytes | 2 | 2588672.0 | 2572288 | 2605056 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | host_sampler.final_bytes | 2 | 6414336.0 | 6373376 | 6455296 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | host_sampler.maximum_gap_ns | 2 | 12522750.0 | 12522208 | 12523292 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | host_sampler.sample_count | 2 | 9.0 | 8 | 10 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | host_sampler.sampled_peak_bytes | 2 | 10665984.0 | 10534912 | 10797056 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | inplace_edit_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | input.fixture_bytes | 2 | 525112450.0 | 525112450 | 525112450 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | input.regular_files | 2 | 100500.0 | 100500 | 100500 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | interrupted_syscall_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | lifecycle.anchor_prefetch_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | lifecycle.cleanup_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | lifecycle.docker_calls | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | lifecycle.docker_setup_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | lifecycle.helper_copy_ns | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | lifecycle.mount_ready_ns | 2 | 11509895.5 | 8640791 | 14379000 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | lifecycle.proxy_ns | 2 | 194395.5 | 175041 | 213750 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | lifecycle.small_file_prefetch_bytes | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | lifecycle.small_file_prefetch_eligible | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | lifecycle.snapshot_cache_bytes_at_create | 2 | 1878.0 | 1376 | 2380 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | lifecycle.snapshot_cache_rows_at_create | 2 | 9.5 | 9 | 10 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | lifecycle.snapshot_database_bytes | 2 | 12580.0 | 11172 | 13988 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | lifecycle.snapshot_database_calls | 2 | 12.0 | 12 | 12 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | lifecycle.snapshot_database_rows | 2 | 12.0 | 12 | 12 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | lifecycle.snapshot_store_wide_scans | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | lifecycle.total_ns | 2 | 16183396.0 | 15585792 | 16781000 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | lifecycle.unattributed_ns | 2 | 281229.5 | 239334 | 323125 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | lifecycle.unmount_ns | 2 | 3136625.0 | 759958 | 5513292 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | lifecycle.wait_ns | 2 | 1061250.5 | 1017334 | 1105167 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | metadata_normalization_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | orchestration_unattributed_ns | 2 | 32113395.5 | 29602667 | 34624124 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | preparation_ns | 2 | 4509170271.0 | 4440686792 | 4577653750 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | pure_call_sum_ns | 2 | 28640438.0 | 25590208 | 31690668 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | runtime_preparation_ns | 2 | 406764187.5 | 386352125 | 427176250 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | spool_boundary.max_allocated_bytes | 2 | 4096.0 | 4096 | 4096 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | spool_boundary.max_file_count | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | spool_boundary.max_logical_bytes | 2 | 139.0 | 139 | 139 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | store.allocated_bytes.delta | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | store.allocated_bytes.end | 2 | 673447936.0 | 673251328 | 673644544 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | store.allocated_bytes.max | 2 | 673447936.0 | 673251328 | 673644544 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | store.allocated_bytes.start | 2 | 673447936.0 | 673251328 | 673644544 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | store.file_bytes.delta | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | store.file_bytes.end | 2 | 673447936.0 | 673251328 | 673644544 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | store.file_bytes.max | 2 | 673447936.0 | 673251328 | 673644544 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | store.file_bytes.start | 2 | 673447936.0 | 673251328 | 673644544 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | store.freelist_page_count.delta | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | store.freelist_page_count.end | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | store.freelist_page_count.max | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | store.freelist_page_count.start | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | store.live_page_bytes.delta | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | store.live_page_bytes.end | 2 | 673447936.0 | 673251328 | 673644544 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | store.live_page_bytes.max | 2 | 673447936.0 | 673251328 | 673644544 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | store.live_page_bytes.start | 2 | 673447936.0 | 673251328 | 673644544 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | store.page_count.delta | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | store.page_count.end | 2 | 10276.0 | 10273 | 10279 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | store.page_count.max | 2 | 10276.0 | 10273 | 10279 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | store.page_count.start | 2 | 10276.0 | 10273 | 10279 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | verified.canonical-verification.canonical_Chunk_bytes | 2 | 527393254.5 | 527392728 | 527393781 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | verified.canonical-verification.canonical_Chunk_objects | 2 | 108609.0 | 108584 | 108634 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | verified.canonical-verification.canonical_DirectoryNode_bytes | 2 | 4468273.0 | 4468273 | 4468273 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | verified.canonical-verification.canonical_DirectoryNode_objects | 2 | 1024.0 | 1024 | 1024 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | verified.canonical-verification.canonical_DirectoryState_bytes | 2 | 63112.0 | 63112 | 63112 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | verified.canonical-verification.canonical_DirectoryState_objects | 2 | 644.0 | 644 | 644 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | verified.canonical-verification.canonical_FileNode_bytes | 2 | 8764138.0 | 8763072 | 8765204 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | verified.canonical-verification.canonical_FileNode_objects | 2 | 100449.5 | 100448 | 100451 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | verified.canonical-verification.canonical_FileState_bytes | 2 | 10647647.0 | 10647488 | 10647806 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | verified.canonical-verification.canonical_FileState_objects | 2 | 100449.5 | 100448 | 100451 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | verified.canonical-verification.canonical_InodeRecord_bytes | 2 | 9906869.0 | 9906722 | 9907016 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | verified.canonical-verification.canonical_InodeRecord_objects | 2 | 101090.5 | 101089 | 101092 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | verified.canonical-verification.canonical_InodeTable_bytes | 2 | 6595624.0 | 6594328 | 6596920 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | verified.canonical-verification.canonical_InodeTable_objects | 2 | 1134.0 | 1122 | 1146 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | verified.canonical-verification.canonical_Metadata_bytes | 2 | 286.0 | 286 | 286 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | verified.canonical-verification.canonical_Metadata_objects | 2 | 2.0 | 2 | 2 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | verified.canonical-verification.canonical_Namespace_bytes | 2 | 121.0 | 121 | 121 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | verified.canonical-verification.canonical_Namespace_objects | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | verified.canonical-verification.canonical_unique_bytes | 2 | 567839324.5 | 567836130 | 567842519 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | verified.canonical-verification.canonical_unique_objects | 2 | 413403.5 | 413362 | 413445 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | verified.canonical-verification.independent_content_paths | 2 | 100500.0 | 100500 | 100500 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | verified.canonical-verification.logical_bytes | 2 | 525112450.0 | 525112450 | 525112450 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | verified.canonical-verification.persistence_custody_paths | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | verified.canonical-verification.verified_paths | 2 | 101144.0 | 101144 | 101144 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | verified.canonical-verification.verified_regular_paths | 2 | 100500.0 | 100500 | 100500 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | visibility_ns | 2 | 118916.5 | 98833 | 139000 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | visited_file_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | visited_path_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | workload_chmod_call_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | workload_close_call_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | workload_closedir_call_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | workload_fsync_call_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | workload_fsyncdir_call_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | workload_ftruncate_call_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | workload_lstat_call_count | 2 | 1.0 | 1 | 1 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | workload_mkdir_call_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | workload_ns | 2 | 2556083.5 | 2300250 | 2811917 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | workload_open_call_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | workload_open_directory_call_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | workload_opendir_call_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | workload_plan_ns | 2 | 861667.0 | 813917 | 909417 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | workload_pread_call_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | workload_pwrite_call_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | workload_rename_call_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | workload_rmdir_call_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | workload_symlink_call_count | 2 | 0.0 | 0 | 0 |
| corrected / `39b70a103d99f422` | tiny-stat-1 | workload_unlink_call_count | 2 | 0.0 | 0 | 0 |

## Per-step curves and sharing denominators

[step-evidence.json](step-evidence.json) retains every eligible sample's per-step public timings, published root, Commit/FUSE/candidate observations, Store endpoints/deltas, matching canonical role census, per-variant CDC evidence and retained-history union accounting. Genesis is step0; measured operation ordinal0 joins verified snapshot1. Current-state and retained-union gauges stay distinct. Regular payload sharing excludes metadata, canonical wrappers and Store slack; addition-only and retained-history denominators are explicit.

| Case | Seed | Arm / source group | Step | Commit ns | Store growth this step | New payload bytes | Retained payload bytes | Retained logical bytes | Retained canonical bytes |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |

## CLI invocation wall

A family invocation can cover many samples. Its full CLI wall is not copied into each sample or added to sample wall. Interrupted invocation wall remains unknown.

| Source | Arm | Selected slots | Full CLI ns | Source validation ns | Registry ns |
| --- | --- | ---: | ---: | ---: | ---: |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 5028628041 | 285280500 | 6541250 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 15203231958 | 309903417 | 9508041 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 14992205459 | 300357875 | 6039208 |
| `f5f8a69859bd9c0a2e7dc7780de55578fb05eec3` | corrected | 1 | 4177193458 | 323132917 | 6944791 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 14184027667 | 298830416 | 6304916 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 25 | 43723382500 | 860819916 | 6415625 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 10992513292 | 288960250 | 6456417 |
| `f5f8a69859bd9c0a2e7dc7780de55578fb05eec3` | corrected | 1 | 18441997041 | 299591417 | 6371125 |
| `f5f8a69859bd9c0a2e7dc7780de55578fb05eec3` | corrected | 1 | 17675112416 | 331783459 | 7470083 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 16821839166 | 279437875 | 6540209 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 5451063459 | 276795000 | 6298042 |
| `f5f8a69859bd9c0a2e7dc7780de55578fb05eec3` | corrected | 1 | 21183690792 | 306497500 | 8308791 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 24 | 63439655709 | 810460041 | 8720208 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 5622388875 | 296955125 | 6347083 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 6425301292 | 292246125 | 6737750 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 5764785291 | 288014209 | 6259417 |
| `e7840da1da81404ff228be734a91783cebb946ca` | corrected | 1 | 611788092750 | 794334666 | 3669123709 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 60 | 180587740875 | 967788083 | 7454250 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 4972236416 | 307065834 | 6622417 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 5720149833 | 282078625 | 6099750 |
| `f5f8a69859bd9c0a2e7dc7780de55578fb05eec3` | corrected | 1 | 23573136417 | 300265916 | 6514958 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 16625378875 | 295720958 | 6763042 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 4822381042 | 281293542 | 6407459 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 3782468291 | 297107500 | 6352000 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 11000042125 | 288196709 | 6260792 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 6813774750 | 298294625 | 6183250 |
| `f5f8a69859bd9c0a2e7dc7780de55578fb05eec3` | corrected | 1 | 20858828083 | 290297584 | 6759458 |
| `f5f8a69859bd9c0a2e7dc7780de55578fb05eec3` | corrected | 1 | 3786490250 | 301104959 | 7042334 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 48 | 124164061542 | 288534584 | 6059208 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 4984157042 | 289324416 | 6193875 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 60 | 12114108291 | 865695125 | 7189375 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 60 | 395349654625 | 4123283292 | 8201250 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 12 | 83662650666 | 926842625 | 8884041 |
| `f5f8a69859bd9c0a2e7dc7780de55578fb05eec3` | corrected | 1 | 21812325625 | 284116125 | 6315708 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 5862995709 | 293733042 | 6789292 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 4225648792 | 285488000 | 6503167 |
| `f5f8a69859bd9c0a2e7dc7780de55578fb05eec3` | corrected | 1 | 17876025375 | 294262625 | 6409916 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 4774291417 | 295912417 | 6727541 |
| `f5f8a69859bd9c0a2e7dc7780de55578fb05eec3` | corrected | 1 | 18309730416 | 403373375 | 6685958 |
| `f5f8a69859bd9c0a2e7dc7780de55578fb05eec3` | corrected | 1 | 18864826500 | 292640000 | 6940875 |
| `f5f8a69859bd9c0a2e7dc7780de55578fb05eec3` | corrected | 1 | 3652010125 | 299950042 | 6872625 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 36 | 95705753750 | 814495500 | 6764583 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 3591204250 | 297710750 | 7084833 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 60 | 249461719125 | 835598000 | 8193167 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 3584027458 | 295633833 | 6342084 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 6394500875 | 297402375 | 6458709 |
| `f5f8a69859bd9c0a2e7dc7780de55578fb05eec3` | corrected | 60 | 2574452928417 | 307839833 | 6492000 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 3750306292 | 298889375 | 6554833 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 5494355625 | 281899292 | 6378208 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 6302123208 | 900578375 | 6495333 |
| `f5f8a69859bd9c0a2e7dc7780de55578fb05eec3` | corrected | 1 | 22561209875 | 302493709 | 6657542 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 30 | 81396174583 | 858935584 | 6848792 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 3719539125 | 303020083 | 6762667 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 5678767042 | 285419709 | 6287708 |
| `f5f8a69859bd9c0a2e7dc7780de55578fb05eec3` | corrected | 1 | 7976498541 | 303450417 | 7422500 |
| `f5f8a69859bd9c0a2e7dc7780de55578fb05eec3` | corrected | 1 | 18061281333 | 299683041 | 6547917 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 6785668750 | 295795250 | 6339458 |
| `f5f8a69859bd9c0a2e7dc7780de55578fb05eec3` | corrected | 1 | 7221070375 | 321198083 | 6796125 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 14767634125 | 289682708 | 6379208 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 3689818583 | 301207959 | 6725542 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 16615215750 | 304501709 | 7046625 |
| `f5f8a69859bd9c0a2e7dc7780de55578fb05eec3` | corrected | 1 | 7129439875 | 297928209 | 6498208 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 11851560959 | 894531750 | 7712833 |
| `f5f8a69859bd9c0a2e7dc7780de55578fb05eec3` | corrected | 1 | 17635433000 | 300832750 | 6478083 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 14818199042 | 297346000 | 6427250 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 10959361750 | 843702833 | 6761333 |
| `f5f8a69859bd9c0a2e7dc7780de55578fb05eec3` | corrected | 1 | 18607423417 | 302818334 | 6440875 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 6673584292 | 282804166 | 6612375 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 11116780417 | 319935166 | 6711459 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 14508290459 | 314245834 | 6564209 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 3795214875 | 301365708 | 6404792 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 5742255458 | 290203041 | 6398833 |
| `f5f8a69859bd9c0a2e7dc7780de55578fb05eec3` | corrected | 1 | 20937594541 | 323483458 | 7023417 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 4767812875 | 281383167 | 6657584 |
| `f5f8a69859bd9c0a2e7dc7780de55578fb05eec3` | corrected | 1 | 7380765917 | 2266396500 | 7771083 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 30 | 154157509042 | 793549000 | 6243375 |
| `f5f8a69859bd9c0a2e7dc7780de55578fb05eec3` | corrected | 1 | 18512794375 | 301123500 | 6731667 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 3830634500 | 298630416 | 6446542 |
| `7948df2de269e5ffd47a232ffd8091ff83f8869f` | corrected | 1 | 6301605833 | 328162792 | 7101500 |
| `f5f8a69859bd9c0a2e7dc7780de55578fb05eec3` | corrected | 1 | 3729623959 | 303099584 | 6962292 |

## Failures and remaining evidence work

- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-100m-s1-performance-7da063b41104` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-100m-s2-performance-dc56c2b1acb1` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-100m-s3-performance-1dcb01d5c59d` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-10m-s1-performance-912dad86209a` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-10m-s2-performance-ff53d26cf2f5` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-10m-s3-performance-344cfcee6a91` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-1m-s1-performance-5a93ab533372` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-1m-s2-performance-7352299acb92` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-1m-s3-performance-a46b514b9b9e` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-500m-s1-performance-1a69f15c45d9` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-500m-s2-performance-18e28e21b8e8` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-500m-s3-performance-4d91790276b3` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-random-read-100-s1-performance-55b1f48777d3` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-random-read-100-s2-performance-38dfc033e7e6` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-random-read-100-s3-performance-e781668bd22f` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-random-read-10-s1-performance-7c9c9550a2b3` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-random-read-10-s2-performance-6c8472421999` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-random-read-10-s3-performance-2b9a4403b328` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-random-read-1-s1-performance-25e14af7799c` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-random-read-1-s2-performance-94c426755a67` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-random-read-1-s3-performance-03f2ecd71f89` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-random-read-500-s1-performance-43073ffac5e1` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-random-read-500-s2-performance-21f7593ee99c` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-random-read-500-s3-performance-c29c1382be39` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-100-s1-performance-6e322f16632d` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-100-s2-performance-15931b808655` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-100-s3-performance-7775d37b9f21` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-10-s1-performance-4b2bf902915f` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-10-s2-performance-b2a48eee9190` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-10-s3-performance-67c38fcebd50` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-1-s1-performance-82a960b7319c` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-1-s2-performance-a3052c40d6c9` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-1-s3-performance-c3106124dcdc` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-500-s1-performance-42d284ee49b7` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-500-s2-performance-462a9388dc2f` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-500-s3-performance-fcfeb1c9523c` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-100-s1-performance-ba27c14fa2bf` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-100-s2-performance-4505aea3ab4f` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-100-s3-performance-a7fa9e544dc7` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-10-s1-performance-84841568f52e` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-10-s2-performance-df7c2dc19cec` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-10-s3-performance-7976810fdba9` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-1-s1-performance-cc1b908f2749` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-1-s2-performance-e8f7ec62486d` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-1-s3-performance-f69793436a19` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-500-s1-performance-cf64c1d0ecbe` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-500-s2-performance-3e206a75b5a0` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-500-s3-performance-ad9fbdc9b047` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-100-s1-performance-1d79a2cb81c7` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-100-s2-performance-d9ccdfae1ce7` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-100-s3-performance-c5bea0e4f02e` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-10-s1-performance-40288364827f` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-10-s2-performance-a4c4d458bfc5` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-10-s3-performance-962fec47bc17` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-1-s1-performance-3a3f77d6ca0c` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-1-s2-performance-3ad81dad4042` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-1-s3-performance-3d156aa6829b` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-500-s1-performance-e1245cd1d674` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-500-s2-performance-f9efc1ed8c63` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-500-s3-performance-31bf67cc4035` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-100-s1-performance-da53ec7aa9e2` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-100-s2-performance-050073c1ce5f` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-100-s3-performance-677881af12bb` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-10-s1-performance-e5e40d8716c6` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-10-s2-performance-73ee9538228f` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-10-s3-performance-c19770d7871d` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-1-s1-performance-c1b4b2b924ad` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-1-s2-performance-e07aca40d5ed` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-1-s3-performance-d903c0637fa3` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-500-s1-performance-cd5493c09450` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-500-s2-performance-6816d22336ad` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-500-s3-performance-6d62be8523e8` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-100-s1-performance-08dea611a747` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-100-s2-performance-d19dbe985993` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-100-s3-performance-e0db15041bc4` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-10-s1-performance-46d111390331` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-10-s2-performance-149e961df39d` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-10-s3-performance-bd48132d46c0` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-1-s1-performance-d4842bcaeaf8` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-1-s2-performance-9c091dae1c72` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-1-s3-performance-76f85e2a5b82` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-500-s1-performance-2003a900c0b8` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-500-s2-performance-62587635ae1a` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-500-s3-performance-7e072bd04e8b` — Functional repairs alter write/structural paths and PieceTree/Data/Node memory layout (including readonly lifecycle/resources). Preserve original baseline; collect separately labeled corrected candidate. No source relabel.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-1-s1-performance-2babc4ee0210` — Sampler emitted a partial final row at container shutdown; mandatory observation is invalid. Atomic whole-row sampler repair; recollect only this source/case/seed/mode, preserve original raw product pass and invalid observation.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-100-s1-performance-e66c9d483f07` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-100-s2-performance-b14c2de7bc19` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-100-s3-performance-cf305e573b3a` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-10-s1-performance-bdf49623648f` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-10-s2-performance-dc2a7fb14e0d` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-10-s3-performance-70095c007a8d` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-1-s1-performance-ce1e105a36dd` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-1-s2-performance-64e862866af2` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-1-s3-performance-ead427bcace4` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-500-s1-performance-f02cb09f7d14` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-500-s1-verify-4ed93a7acfd4` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-500-s2-performance-0710bba4a5ea` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-500-s3-performance-78174e67a5c9` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-100-s1-performance-a81c102a0e47` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-100-s2-performance-d2c72c9d2d54` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-100-s3-performance-af9ed587b82c` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-10-s1-performance-af22f1716374` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-10-s2-performance-be2fe31fef62` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-10-s3-performance-16a52ce72aa0` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-1-s1-performance-2de5c0a99dcf` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-1-s2-performance-1afeabeca7ca` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-1-s3-performance-2fff5d8336d7` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-500-s1-performance-5435dcc59b91` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-500-s2-performance-6545bfbd37ca` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-500-s3-performance-684c21dbd5ee` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/git-tool-10-s1-performance-5abd0cdea1ba` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/git-tool-1-s1-performance-a7a8dcacb59f` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/git-tool-1-s2-performance-76cc23a1e15a` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/git-tool-1-s3-performance-d437712383b8` — Unlink-only functional repair0763fac6 changes acknowledged deletion semantics/work. Conservatively recollect all prior deleting/Git performance and bulk-delete proof; original PASS/FAIL outcomes remain source-bound.96 zero-unlink/rmdir passes and payload proof retained by explicit exact-source bridge.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-dense-rewrite-100-s1-performance-7fb7938ebfff` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-dense-rewrite-100-s2-performance-7088546aa22c` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-dense-rewrite-100-s3-performance-e1d9ce0bb8ef` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-dense-rewrite-500-s1-performance-0b53858b44b7` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/namespace-subtree-relocate-delete-500-s1-performance-6a046f9b5838` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/namespace-subtree-relocate-delete-500-s2-performance-18608b6eec7b` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/namespace-subtree-relocate-delete-500-s3-performance-13cbd2dc256b` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-clean-commit-100-s1-performance-af2e5ecfb3da` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-clean-commit-100-s2-performance-0e14bdbe7d04` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-clean-commit-100-s3-performance-6715f51dfd6f` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-clean-commit-10-s1-performance-eb68f9297ee5` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-clean-commit-10-s2-performance-d6bd3ace97fa` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-clean-commit-10-s3-performance-1ecf098b67de` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-clean-commit-1-s1-performance-13039c8c7de8` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-clean-commit-1-s2-performance-4e22318ce4b7` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-clean-commit-1-s3-performance-a9d0db7dbeb0` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-clean-commit-500-s1-performance-79d7ca0233ac` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-clean-commit-500-s2-performance-e2423789d54e` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-clean-commit-500-s3-performance-1c3cedfe2288` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-dense-rewrite-10-s1-performance-19cea7c1bb97` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-dense-rewrite-10-s2-performance-b229fbb5152c` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-dense-rewrite-10-s3-performance-ba163d7f42a1` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-dense-rewrite-1-s1-performance-ee3b1d1bce09` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-dense-rewrite-1-s2-performance-20d588cb4f63` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-dense-rewrite-1-s3-performance-2b510e8006dc` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-distributed-sdk-edit-100-s1-performance-36c7beedab88` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-distributed-sdk-edit-100-s2-performance-c3ee76a5b586` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-distributed-sdk-edit-100-s3-performance-1fa5fdbadd52` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-distributed-sdk-edit-10-s1-performance-4b0e9d467914` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-distributed-sdk-edit-10-s2-performance-6ab58977b954` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-distributed-sdk-edit-10-s3-performance-19570b3d01f3` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-distributed-sdk-edit-1-s1-performance-24985773c58d` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-distributed-sdk-edit-1-s2-performance-93f391eee444` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-distributed-sdk-edit-1-s3-performance-f0be1e64d719` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-distributed-sdk-edit-500-s1-performance-551e85fc5e1b` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-distributed-sdk-edit-500-s2-performance-fd8f1f651424` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-distributed-sdk-edit-500-s3-performance-b59d9a45bf0b` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-fixed-move-100-s1-performance-03275bc8a3cf` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-fixed-move-100-s2-performance-2125fdd45aa4` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-fixed-move-100-s3-performance-11463195366b` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-fixed-move-10-s1-performance-b40d7a77616f` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-fixed-move-10-s2-performance-7979e2f99bd5` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-fixed-move-10-s3-performance-a025d65e3fe0` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-fixed-move-1-s1-performance-809f38a422f5` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-fixed-move-1-s2-performance-2ac3432f53f2` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-fixed-move-1-s3-performance-5c0fa60e178d` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-fixed-move-500-s1-performance-8d19de8a7f62` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-fixed-move-500-s2-performance-acdc8a9904fc` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/workspace-fixed-move-500-s3-performance-bd351fce91dd` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/git-tool-100-s1-performance-4ba52627e3a9` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/git-tool-100-s2-performance-f581f2d4d6b9` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/git-tool-100-s3-performance-35808ac63a12` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/git-tool-10-s1-performance-e299d86f4cf1` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/git-tool-10-s2-performance-a0f4937a37dd` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/git-tool-10-s3-performance-06e487559b52` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/git-tool-1-s1-performance-f9ea701685c7` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/git-tool-1-s2-performance-0c3796cb8a3d` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/git-tool-1-s3-performance-be649626e5a8` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/git-tool-500-s1-performance-dc5c23c6d822` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/git-tool-500-s2-performance-7595ca76057a` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/git-tool-500-s3-performance-f74dd21a00da` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/namespace-subtree-relocate-delete-100-s1-performance-af25c5355375` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/namespace-subtree-relocate-delete-100-s2-performance-dc5d0be2ba32` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/namespace-subtree-relocate-delete-100-s3-performance-3da6ed534ab5` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/namespace-subtree-relocate-delete-10-s1-performance-c1945ac58f5d` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/namespace-subtree-relocate-delete-10-s2-performance-28b87a35411f` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/namespace-subtree-relocate-delete-10-s3-performance-1aa9f3bcf09f` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/namespace-subtree-relocate-delete-1-s1-performance-c17b24758208` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/namespace-subtree-relocate-delete-1-s2-performance-dc803d683798` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/namespace-subtree-relocate-delete-1-s3-performance-f4a88f8f6bf5` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-100-s1-performance-3cc649b9267a` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-100-s2-performance-f59ad3e7ec74` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-100-s3-performance-a990da42d956` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-10-s1-performance-5babcd58334a` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-10-s2-performance-7a28bffab14b` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-10-s3-performance-080b85a71d3c` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-1-s1-performance-e31b47a21ddf` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-1-s2-performance-ee92d257ca77` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-1-s3-performance-ff522ac55ab4` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-500-s1-performance-2a012aa6c4c0` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-500-s2-performance-732c82ac8c30` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-500-s3-performance-f983199af8ec` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-100-s1-performance-cc59eaf8d620` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-100-s2-performance-cceb2d0fefc8` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-100-s3-performance-231e0d511bd0` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-10-s1-performance-43acf8bcc94f` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-10-s2-performance-18e10d399131` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-10-s3-performance-8d41bed508f8` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-1-s1-performance-40cdb5183d8b` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-1-s2-performance-cc573cc33bc5` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-1-s3-performance-9e862ca97e9b` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-500-s1-performance-b3aaf4cb1e0e` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-500-s2-performance-b722b606a55c` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-500-s3-performance-6904813a56c6` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/agent-episodes-1-s1-performance-c3675b7ba8b9` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-construct-100-s1-performance-30a23e8c2b4c` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-construct-100-s2-performance-d0caee7c12cd` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-construct-100-s3-performance-538b20b2b416` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-construct-10-s1-performance-b551ce82a3e2` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-construct-10-s2-performance-d95d06629df1` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-construct-10-s3-performance-fc971e031b4f` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-construct-1-s1-performance-ae1c9c2d9e2f` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-construct-1-s2-performance-f3b526776b62` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-construct-1-s3-performance-3153221b6b32` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-construct-500-s1-performance-718bcb581e1c` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-construct-500-s2-performance-18e52c145593` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-construct-500-s3-performance-f14180783c03` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-content-scan-100-s1-performance-a2270e1502a3` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-content-scan-100-s2-performance-b35a6215e9c4` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-content-scan-100-s3-performance-76c7768f6fc1` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-content-scan-10-s1-performance-4d767d6aff5c` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-content-scan-10-s2-performance-bf3ae46b4e7d` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-content-scan-10-s3-performance-a0d6d5afdb30` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-content-scan-1-s1-performance-3957914bf567` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-content-scan-1-s2-performance-4f21dcde750e` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-content-scan-1-s3-performance-1e4ccf070a70` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-content-scan-500-s1-performance-28088677ee20` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-content-scan-500-s2-performance-5b2c59ebb5af` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-content-scan-500-s3-performance-619e2b21e412` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-metadata-scan-100-s1-performance-614710a9ab30` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-metadata-scan-100-s2-performance-372354650b3a` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-metadata-scan-100-s3-performance-d03d87270fa3` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-metadata-scan-10-s1-performance-e2a3f4667120` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-metadata-scan-10-s2-performance-a2104b0b686e` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-metadata-scan-10-s3-performance-d2e9cb785515` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-metadata-scan-1-s1-performance-542da696e31a` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-metadata-scan-1-s2-performance-a2ce386f5e5d` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-metadata-scan-1-s3-performance-32f94d8dfcd4` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-metadata-scan-500-s1-performance-a13b9370062e` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-metadata-scan-500-s2-performance-70c134a71a2a` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/directory-metadata-scan-500-s3-performance-06dbdd417d95` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-100m-s1-performance-1fb7b864d1e0` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-100m-s2-performance-f17e65c2b898` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-100m-s3-performance-d6c1927cd0a3` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-10m-s1-performance-fcaa6f893b67` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-10m-s2-performance-32a1e1d0d35b` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-10m-s3-performance-752540500493` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-1m-s1-performance-f285d157909c` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-1m-s2-performance-ea48f9e005ef` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-1m-s3-performance-569400ec4764` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-500m-s1-performance-e571cc2378a3` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-500m-s2-performance-9824cfc762f1` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-500m-s3-performance-1a8583b1fe00` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-random-read-100-s1-performance-64e557a4f9d2` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-random-read-100-s2-performance-9dca37425fac` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-random-read-100-s3-performance-1383618e48d4` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-random-read-10-s1-performance-272dd7017284` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-random-read-10-s2-performance-0040973d829a` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-random-read-10-s3-performance-59aed17c2284` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-random-read-1-s1-performance-b084b4866af1` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-random-read-1-s2-performance-9500d171657b` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-random-read-1-s3-performance-65145683fe47` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-random-read-500-s1-performance-a8caa75df658` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-random-read-500-s2-performance-f2c7416affbe` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-random-read-500-s3-performance-e0ff04efd759` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-100-s1-performance-094f610fbd21` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-100-s2-performance-d65fcdd84088` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-100-s3-performance-b618379b4fd1` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-10-s1-performance-4c30169755f6` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-10-s2-performance-3bf4a1bb9d10` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-10-s3-performance-f0b4727eec56` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-1-s1-performance-04df5363be6d` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-1-s2-performance-e465631f52f2` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-1-s3-performance-7f9f3f5811aa` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-500-s1-performance-668133342791` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-500-s2-performance-dd56a8b819b1` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-500-s3-performance-b59e39bbb7fe` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-100-s1-performance-699a5b3be1d9` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-100-s2-performance-295ac52c21a7` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-100-s3-performance-119ad2e41bfa` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-10-s1-performance-b743f0f0d1e3` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-10-s2-performance-a4f4b45dc1dc` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-10-s3-performance-33e061209ff1` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-1-s1-performance-4c2b396414e0` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-1-s2-performance-b3a7902546c6` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-1-s3-performance-f5416b3d9a7c` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-500-s1-performance-f638abcba3c8` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-500-s2-performance-9887963ea8f9` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-500-s3-performance-3ed6d14f70f8` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-100-s1-performance-b282e8e489bd` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-100-s2-performance-ff6e9ea4dd64` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-100-s3-performance-84545977e370` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-10-s1-performance-077f4908edaf` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-10-s2-performance-8550d92616bb` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-10-s3-performance-1e68bc1b84ca` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-1-s2-performance-6def60a05d9f` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-1-s3-performance-3de2435d8f9d` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-500-s1-performance-01b19b63ba99` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-500-s2-performance-4eed45dd5c7c` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-500-s3-performance-acdc525b72e2` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-1-s1-performance-f95ef696b6f6` — Unrequested feature-enabled SQL_TRACE retained every SQL statement as an unbounded String history inside public-call timers and host process memory. Frozen timing/observation contract does not authorize this recorder. Preserve actual old product outcome and source as diagnostic only; recollect performance after capture defaults off. Canonical qualified inputs are not invalidated by diagnostic history.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/dedup-cross-file-mixed-100-s1-performance-25d1b37de7a3` — Prior attempt was unexecuted at the shared cache cap. Explicit duplicate/cold cache maintenance restored capacity; reuse the already-qualified selected input and preserve the old failure.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/dedup-cdc-insert-500-s2-performance-a1fd1885549c` — Prior attempt stopped before execution at the shared cache cap. Explicit completed-family cold eviction restored capacity; reuse its already-prepared qualified input and preserve the failed attempt.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/dedup-cdc-scattered-500-s3-performance-02ca311ce729` — Previous preparation stopped at24GiB cache cap beforeproduct. Explicit completedCDC cold eviction restored capacity; reuse selected qualified input and preserve old attempt.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/dedup-cross-file-identical-500-s3-performance-32c822f34890` — Retry only the unexecuted cache-cap attempt32c822f34890 after explicit eviction of disposable inputs for suppressed cases; prior actual outcome retained.. Its original product status is unchanged; it cannot support performance claims.
- **Retained invalidated observation**: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/dedup-cdc-scattered-500-s2-performance-b4acce77893e` — Retry only the unexecuted cache-cap attemptb4acce77893e after explicit completed-input eviction; protected remaining seed dependencies and preserved all outcomes.. Its original product status is unchanged; it cannot support performance claims.
- 344 required slots remain missing; review.json contains exact IDs.

## Scope

This is initial benchmark evidence, not release admission. Still-active product failures block Phase1 completion and require repair. Runtime-suppressed coverage is explicitly outside this amended Phase1 scope and is never labelled passing. Historical raw passes and failures remain unchanged in retained_failure_history with diagnostic/invalidation labels; corrected clean-capture outcomes keep separate source identities. Report regeneration does not rerun product work. No cold-cache, optimization or crash/power-loss guarantee is claimed. Issue #21 remains open.
