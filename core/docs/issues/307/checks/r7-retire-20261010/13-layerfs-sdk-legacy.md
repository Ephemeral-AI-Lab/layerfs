# Coverage audit: `layerfs-sdk-legacy`

> **Status:** Dated checkpoint receipt; coverage audit written before any removal.

The previous agent SDK over one composed host Server and its Sandbox owner. Classes and the directions `[D1]`–`[D13]` are defined in [00-scope-and-method.md](00-scope-and-method.md). Test directories: D daemon, W workspace, O overlay, F fuse, B bridge, S sandbox, K SDK, P project, C content, PE persistence.

Verdict: **NOT REMOVED in this stage. It carries pinned read-only view code, which no active crate covers and whose future is the unanswered owner question O-10. Everything else in it is class A or C; the removal is prepared and needs only the owner's answer.**

## Source

Production lines from the pinned counter at `cf5bd62ce` ([receipt 01](01-production-loc-head.txt)). Total 505 in 5 files.

| File | Lines | Behaviour | Class | Covered by / recorded direction |
| --- | ---: | --- | --- | --- |
| `src/lib.rs` | 16 | Exports of ProjectApi, SandboxApi, WorkspaceApi and the composed Server | A+C | `layerfs-api/sdk/src/lib.rs`; the Server re-export is retired [D1] |
| `src/project.rs` | 86 | Project Init and Branch fork through the composed Server | A+C | Server route [D1]. `ProjectApi::init`, `install`, `fork`, `history`: K `init.rs` (2), D `native_install.rs` (3), D `native_control.rs::native_mount_commit_status_fork_history_and_terminal_unmount` |
| `src/sandbox.rs` | 28 | Sandbox create, list, delete, delete with logs through the host owner | A+C | `SandboxApi::create`, `ManagedSandbox` stop/delete: S `engine_lifecycle.rs` (6), example `sandbox_lifecycle.rs`. `list` and log capture are outside the selected scope [D11] |
| `src/workspace.rs` | 175 | Workspace mount, exec, commit, status, unmount routed through the host owner | A+C | `WorkspaceApi` bind/attach/mount, commit, status, unmount, force_unmount: D `native_control.rs` (10), D `native_mount_routes.rs` (2). Managed Exec with its 30 s cap and status polling is dropped [D2]; ordinary execution is `SandboxApi::exec` |
| `src/workspace_view.rs` | 200 | pin_view, view_lookup, view_list, view_read, view_readlink, view_status, release_view | O | Pinned read-only SDK view leases. Not covered by any active crate; whether the product keeps them is the unanswered owner question O-10 (303/08 K20, §4.2, §7) |

Lines by class: A+C 305, O 200.

## Tests

42 test functions in 8 files. None can be built at HEAD ([receipt 02](02-cargo-cannot-load.txt)), so none is coverage today; each is classified by the behaviour it asserted.

| Test | Class | Covered by / recorded direction |
| --- | --- | --- |
| **`tests/agent_route.rs`** (4) | | |
| `sdk_only_lifecycle_edit_commit_readback_history_conflict_and_cleanup` | A+C | Managed Exec and HeadMoved conflict [D2] [D7]. Lifecycle: D `native_control.rs::native_mount_commit_status_fork_history_and_terminal_unmount`, D `mounted_commit.rs::r5_1_external_bash_changes_of_every_kind_are_committed_and_read_back_on_a_fresh_mount` |
| `sdk_exec_reaches_inherited_descendant_beyond_4096_then_commits` | A | D `complete_installed_roots.rs::all_supported_native_paths_survive_install_with_full_oracles`, D `mounted_commit.rs::r5_1_external_bash_changes_of_every_kind_are_committed_and_read_back_on_a_fresh_mount` |
| `sdk_exec_moves_inherited_directory_and_commits_complete_tree` | A | W `captured_namespace_states.rs::a_moved_base_directory_has_no_row_and_no_value`, D `captured_commit.rs::every_kind_of_change_commits_through_the_driver_and_reads_back_from_a_fresh_bind` |
| `sdk_exec_walks_270_components_and_checks_serials_after_commit` | A | W `captured_namespace.rs::a_deep_fresh_chain_ends_in_aliases_and_a_moved_base_directory` |
| **`tests/exec_liveness_control.rs`** (1) | | |
| `silent_exec_exposes_native_progress_boundary` | C | [D2] (daemon Exec progress wire) |
| **`tests/inherited_workspace.rs`** (19) | | |
| `pinned_directory_retains_forgotten_ancestors_and_detached_parent_refuses_mutation` | A | W `native_directory.rs::opened_directory_retains_parent_and_metadata_after_forget_and_rmdir` |
| `base_directory_move_keeps_inherited_children_handles_and_commits` | A | W `captured_namespace_states.rs::a_moved_base_directory_has_no_row_and_no_value`, W `namespace.rs::rename_moves_replaces_and_refuses_cycles_without_descendant_rows` |
| `moved_canonical_symlink_reads_target_by_serial` | A | W `nonfile.rs::linked_symlink_targets_follow_effective_layers_through_repeated_failed_captures` |
| `moved_directory_keeps_frozen_g1_and_later_g2_write` | A | D `captured_commit.rs::mutations_after_the_capture_stay_out_of_its_root_and_survive_install` |
| `growing_move_keeps_cached_descendant_reachable_beyond_4096_bytes` | A | W `captured_namespace.rs::a_deep_fresh_chain_ends_in_aliases_and_a_moved_base_directory` |
| `growing_inherited_move_reaches_uncached_descendant_beyond_4096_bytes` | A | same |
| `growing_rename_refuses_private_budget_before_publication` | C | [D3] [D8] (charged private budget) |
| `a_successful_directory_rename_seals_once_and_refunds_exactly` | C | [D3] (budget refunds of the private index) |
| `growing_prefix_move_has_descendant_independent_private_counts` | A+C | Private counts [D3]. W `namespace.rs::rename_moves_replaces_and_refuses_cycles_without_descendant_rows`, W `namespace_profile.rs::complete_operations_keep_point_work_as_names_and_inodes_grow` |
| `fresh_upper_directory_move_is_the_control` | A | W `captured_namespace_states.rs::a_moved_fresh_directory_keeps_its_serial_and_children` |
| `base_file_move_from_an_unmodified_root_keeps_its_identity` | A | W `captured_namespace.rs::renamed_and_moved_names_of_each_kind_keep_their_inodes` |
| `phase_a_selected_origins_overlap_zero_hardlinks_and_completion_credit` | O | Pinned read-only SDK view leases. Not covered by any active crate; whether the product keeps them is the unanswered owner question O-10 (303/08 K20, §4.2, §7); the non-view half (zero ranges, hard links) is W `captured_namespace_fragments.rs` and W `captured_namespace.rs` |
| `phase_a_lease_admission_frozen_names_and_checked_release` | O | Pinned read-only SDK view leases. Not covered by any active crate; whether the product keeps them is the unanswered owner question O-10 (303/08 K20, §4.2, §7) |
| `phase_a_fresh_nonfile_removal_replacement_and_later_symlink_move` | A | W `captured_namespace_rebound.rs` (8 tests) |
| `phase_a_exact_capability_refuses_before_dependent_inspection` | C | [D1] (Service grants) |
| `phase_a_known_save_and_unknown_commit_keep_custody_without_replay` | A | D `mounted_commit_failures.rs::f3_unknown_history_outcome_keeps_custody_and_the_mount_still_serves` |
| `phase_a_known_canonical_local_failure_resumes_same_selector_once` | A | D `mounted_commit_failures.rs::f4_known_publication_with_an_unattempted_install_keeps_custody`, D `store_commit.rs::known_publication_survives_an_unattempted_local_install` |
| `phase_a_large_payload_append_resize_and_backward_base_copy` | A | W `captured_file_edits.rs::overlapping_writes_shrink_regrow_and_cross_cell_spans_match_final_bytes` |
| `phase_a_namespace_cleanup_error_keeps_published_revision_and_identity` | C | [D3] (cleanup of the private page files) |
| **`tests/init_project.rs`** (3) | | |
| `public_sdk_imports_fresh_100_and_1000_file_fixtures` | A | P `init_sqlite.rs::selected_profiles_init_100_and_1000_pass_the_full_namespace_oracle`, K `init.rs::init_seals_complete_root_and_first_branch_without_retained_handles` |
| `host_setup_exposes_the_same_sdk_init` | C | [D1] (composed host Server) |
| `source_refusals_duplicates_and_concurrent_bindings` | A+C | Server bindings [D1]. K `init.rs::refusal_preserves_existing_output_and_original_request`, P `acquisition_custody.rs::backing_inside_source_is_refused_before_an_operation_begins` |
| **`tests/sandbox_log_capture.rs`** (1) | | |
| `diagnostic_logs_are_bounded_and_capture_failure_does_not_skip_cleanup` | C | [D11] (delete-time log capture is not in the selected SandboxApi scope) |
| **`tests/support/phase_b_commit.rs`** (6) | | |
| `prepare_master` | C | Fixture preparation for the withdrawn host-mediated families [D1] |
| `full_lowering_64mib` | A+C | Host lowering [D9]. D `product_commit_cost.rs::a_small_and_a_large_change_through_the_product_constructor_are_counted` |
| `stage_headroom_2mib` | C | [D3] [D8] (private-disk headroom of staging) |
| `commit_headroom_4mib` | C | [D3] [D8] |
| `native_count_8192` | A | No edit-count cap: W `captured_file_edits.rs::many_edits_use_numbered_records_after_install_and_reads_survive_close` |
| `native_count_10240` | A | same |
| **`tests/support/phase_b_mutations.rs`** (5) | | |
| `mixed_live` | A | D `mounted_concurrency.rs::r6_1_twenty_four_processes_read_and_write_through_three_commits` |
| `mixed_failures` | A | D `mounted_commit_failures.rs` (6 tests) |
| `mixed_local_resume` | A | D `mounted_commit_failures.rs::f4_known_publication_with_an_unattempted_install_keeps_custody` |
| `progress_custody` | C | [D2] (Exec progress custody) |
| `many_commit_diagnostic` | A | D `mounted_cycles.rs::counts_plateau_over_twelve_cycles_with_maintenance_awaited_after_each_unmount` |
| **`tests/support/phase_b_namespace.rs`** (3) | | |
| `move_scaling` | A | W `captured_namespace_cursor.rs::work_follows_the_change_and_not_the_installed_base` |
| `deep_move_back` | A | W `captured_namespace_rebound.rs::a_base_directory_moved_away_and_back_keeps_its_stored_values_with_no_territory_walk` |
| `retained_live_namespace` | A | D `mounted_install.rs::retained_descriptors_and_kernel_caches_are_unchanged_by_install` |

Tests by class: A 25, A+C 4, C 11, O 2.

## Other tracked files

| File | Class | Note |
| --- | --- | --- |
| `Cargo.toml` | C | Depends on `../layerfs-api/core` (removed by F12) and `../layerfs-server`; cannot be loaded |
| `examples/benchmark_init.rs` | A | `layerfs-project/examples/benchmark_init.rs` |
| `examples/benchmark_shell.rs` | C | Driver of the withdrawn host-mediated shell-package family [D1] [D2] |
