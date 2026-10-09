# Coverage audit: `layerfs-workspace-legacy`

> **Status:** Dated checkpoint receipt; coverage audit written before any removal.

The previous Workspace over private page files. Source is classified by directory, with the files that differ listed singly. Classes and the directions `[D1]`–`[D13]` are defined in [00-scope-and-method.md](00-scope-and-method.md). Test directories: D daemon, W workspace, O overlay, F fuse, B bridge, S sandbox, K SDK, P project, C content, PE persistence.

Verdict: **NOT REMOVED in this stage. It carries pinned read-only view code, which no active crate covers and whose future is the unanswered owner question O-10. Everything else in it is class A or C; the removal is prepared and needs only the owner's answer.**

## Source

Production lines from the pinned counter at `cf5bd62ce` ([receipt 01](01-production-loc-head.txt)). Total 26835 in 101 files.

| File | Lines | Behaviour | Class | Covered by / recorded direction |
| --- | ---: | --- | --- | --- |
| `src/backing/` (47 files not listed singly) | 13866 | Private page files, the binary B+ tree, generation packs, charged budgets, ownership ledger and reclamation of the private backing | C | [D3]: deleted outright (13,866 lines per 303/07 §4.3). State lives in the daemon's SQLite overlay: O `engine.rs` (12), O `payload.rs` (4), O `orphans.rs` (11), O `lifetimes.rs` (3), O `reclaim_cost.rs` (3) |
| `src/commit/` (10 files not listed singly) | 3284 | Capture lowering, host upload of final file sequences, prepared-stream construction, staged and composite Commit, local reconciliation | A+C | Host-side construction and upload [D1] [D9]; staged/composite split and HeadMoved [D7]. Commit in the daemon: W `captured_namespace*.rs` (76 tests), W `captured_file_edits.rs` (14), D `store_commit.rs` (8), D `product_commit.rs` (11), D `mounted_commit.rs` (7), D `mounted_commit_failures.rs` (6), D `mounted_install.rs` (5) |
| `src/filesystem/` (22 files not listed singly) | 3711 | Filesystem semantics over the private active index: create, mkdir, mknod, link, symlink, unlink, rmdir, rename, open, read, write, resize, attributes, directory listing | A+C | Behaviour kept and rewritten (303/07 §4.1) over the overlay: W `namespace.rs` (5), W `namespace_view.rs` (5), W `payload.rs` (4), W `native_visit.rs` (6), W `native_directory.rs` (4), W `nonfile.rs` (3), D `native_mutation.rs` (2), D `native_coherence.rs` (6). The active-index mechanism is [D3] |
| `src/overlay/` (4 files not listed singly) | 993 | Generation-local directory deltas, extent pieces and the private capture | A+C | Mechanism [D3]. O `captured_namespace.rs` (5), O `captured_runs.rs` (6), O `frontier.rs` (3), O `payload_runs.rs` (4) |
| `src/runtime/` (4 files not listed singly) | 868 | Workspace state, node table, attachment custody, projection coherence, host calls and lifecycle | A+C | Kept and rewritten (303/07 §4.1): `layerfs-workspace/src/` plans plus `layerfs-daemon` owner and registry; D `owner.rs` (12), D `native_mount.rs` (3), D `mounted_drain.rs` (5). Per-mutation kernel notification [D4], blocking reply permits and deadlines [D5], calls to the host Server [D1] |
| `src/commit/source.rs` | 218 | Bridge Source over captured replacement spans | C | [D9] |
| `src/commit/stream.rs` | 347 | Prepared update as a replayable stream of rows | C | [D9] |
| `src/commit/upload.rs` | 147 | Fixed-window upload of the captured file sequence to the host | C | [D1] [D9] |
| `src/commit_types.rs` | 73 | Staged and composite Commit outcomes | A+C | Bridge `control` replies and SDK `OperationFailure`. Staged/composite split [D7] |
| `src/filesystem/active_view.rs` | 309 | Name resolution and bounded listing against one pinned active revision | O | Pinned read-only SDK view leases. Not covered by any active crate; whether the product keeps them is the unanswered owner question O-10 (303/08 K20, §4.2, §7). Ordinary listing is W `namespace_view.rs` |
| `src/filesystem/namespace_view.rs` | 407 | View-bound namespace resolution for mounted lookups | A+C | W `namespace_view.rs` (5 tests); the lease-bound entry points are O-10 |
| `src/filesystem/projection_counters.rs` | 71 | Per-operation counts of callbacks and upstream calls | A | `layerfs-fuse/src/request/accounting.rs`; D `read_cost.rs`, D `directory_cost.rs` |
| `src/filesystem/view_reads.rs` | 411 | Public read-only view leases over one pinned selected view | O | Pinned read-only SDK view leases. Not covered by any active crate; whether the product keeps them is the unanswered owner question O-10 (303/08 K20, §4.2, §7) |
| `src/lib.rs` | 33 | Exports, including the page-store test seam and view-lease types | A+C | `layerfs-workspace/src/lib.rs`. The `sequence` seam belongs to the private backing [D3]; the view exports are O-10 |
| `src/runtime/attachment.rs` | 262 | Failed attachment custody and explicit cleanup | A | D `native_mount.rs::attach_refusals_have_no_effect_and_a_failed_attach_returns_to_unattached`, D `cleanup_control.rs` |
| `src/runtime/coherence.rs` | 593 | Projection binding, bounded reply exclusion, checked mutation completion and notification | A+C | `layerfs-fuse/src/coherence/`: D `native_coherence.rs` (6). Notification and the reply gate [D4] [D5] |
| `src/runtime/host.rs` | 629 | Calls to the host Server for Save, construction and history | C | [D1]. Its view-lease host calls are O-10 |
| `src/runtime/view_leases.rs` | 133 | Charged registry of held public read-only view leases (32 views) | O | Pinned read-only SDK view leases. Not covered by any active crate; whether the product keeps them is the unanswered owner question O-10 (303/08 K20, §4.2, §7) |
| `src/types.rs` | 480 | Portable request and result types, limits (128 handles, 8 MiB budget, 4 GiB file) | A+C | `layerfs-workspace/src/` types. The fixed limits are retirement targets [D8] |

Lines by class: A 333, A+C 10442, C 15207, O 853.

## Tests

204 test functions in 27 files. None can be built at HEAD ([receipt 02](02-cargo-cannot-load.txt)), so none is coverage today; each is classified by the behaviour it asserted.

| Test | Class | Covered by / recorded direction |
| --- | --- | --- |
| **`tests/attachment.rs`** (9) | | |
| `exact_attachment_observation_preserves_the_existing_owner` | A | D `native_mount_routes.rs::lost_bind_and_attach_acknowledgements_are_located_without_replay` |
| `active_attachment_is_observable_and_refuses_cleanup_without_holding_registry` | A | D `cleanup_control.rs::explicit_cleanup_observes_after_registry_removal_without_recreating_a_binding` |
| `unacquired_failure_and_expired_admission_leave_no_attachment` | A+C | Deadline [D5]. D `native_mount.rs::attach_refusals_have_no_effect_and_a_failed_attach_returns_to_unattached` |
| `attachment_retained` | A | D `native_custody.rs::an_unmount_that_cannot_revoke_stops_retained_and_later_replies_repeat_it` |
| `attachment_substitution` | A | D `mounted_cycles.rs::simultaneous_mounts_over_related_roots_are_isolated_and_a_stale_token_never_redirects` |
| `attachment_concurrent_cleanup` | A | D `cleanup_control.rs` |
| `attachment_cleanup_deadline` | C | [D5] |
| `attachment_branch_capacity` | C | [D12] |
| `attachment_mount_substitution` | A | D `mounted_cycles.rs::simultaneous_mounts_over_related_roots_are_isolated_and_a_stale_token_never_redirects` |
| **`tests/backing_ownership.rs`** (6) | | |
| `routine_reclamation_does_not_rescan_earlier_acquisitions` | A+C | Private ownership ledger [D3]; write-triggered reclamation is withdrawn (303/08 K18). Overlay reclamation: O `reclaim_cost.rs` (3), O `lifetimes.rs` (3), D `owner.rs::last_release_starts_automatic_cleanup_during_idle_and_unrelated_live_work` |
| `released_inputs_are_reclaimed_by_the_next_write` | A+C | Private ownership ledger [D3]; write-triggered reclamation is withdrawn (303/08 K18). Overlay reclamation: O `reclaim_cost.rs` (3), O `lifetimes.rs` (3), D `owner.rs::last_release_starts_automatic_cleanup_during_idle_and_unrelated_live_work` |
| `simultaneous_last_owner_drops_leave_one_release_candidate` | A+C | Private ownership ledger [D3]; write-triggered reclamation is withdrawn (303/08 K18). Overlay reclamation: O `reclaim_cost.rs` (3), O `lifetimes.rs` (3), D `owner.rs::last_release_starts_automatic_cleanup_during_idle_and_unrelated_live_work` |
| `a_failed_release_preserves_later_release_candidates` | A+C | Private ownership ledger [D3]; write-triggered reclamation is withdrawn (303/08 K18). Overlay reclamation: O `reclaim_cost.rs` (3), O `lifetimes.rs` (3), D `owner.rs::last_release_starts_automatic_cleanup_during_idle_and_unrelated_live_work` |
| `refused_admission_preserves_its_existing_owners` | A+C | Private ownership ledger [D3]; write-triggered reclamation is withdrawn (303/08 K18). Overlay reclamation: O `reclaim_cost.rs` (3), O `lifetimes.rs` (3), D `owner.rs::last_release_starts_automatic_cleanup_during_idle_and_unrelated_live_work` |
| `reader_and_owner_pins_survive_routine_maintenance` | A+C | Private ownership ledger [D3]; write-triggered reclamation is withdrawn (303/08 K18). Overlay reclamation: O `reclaim_cost.rs` (3), O `lifetimes.rs` (3), D `owner.rs::last_release_starts_automatic_cleanup_during_idle_and_unrelated_live_work` |
| **`tests/coherence.rs`** (8) | | |
| `coherence_visibility` | A | D `native_mutation.rs::namespace_mutations_are_visible_at_once_and_refusals_stay_refused` |
| `coherence_read_race` | A+C | Reply gate [D5]. D `mounted_concurrency.rs::readers_on_two_workspaces_read_exact_bytes_while_one_is_written` |
| `coherence_exec` | B | Execution of a program stored in the Workspace. **Migrated**: D `mounted_execute.rs::a_stored_program_executes_and_its_in_place_rewrite_executes_next` (added in this stage; Linux receipt `execute-attempt1-linux-mounted_execute.txt`) |
| `coherence_native_save` | A | D `mounted_install.rs::retained_descriptors_and_kernel_caches_are_unchanged_by_install` |
| `coherence_completion` | C | [D4] (notification completion) |
| `coherence_deadline` | C | [D5] |
| `coherence_notify_failure` | C | [D4] |
| `coherence_truncate_failure` | A+C | [D4]. D `native_coherence.rs::size_never_shrinks_under_concurrent_append_and_refusals_stay_refused` |
| **`tests/commit_overlap.rs`** (3) | | |
| `a_frozen_walk_page_read_is_held_with_the_writer_gate_free` | A+C | Writer gate of the private index [D3]. D `store_commit.rs::a_held_producer_does_not_hold_workspace_metadata_or_read_capacity`, W `captured_namespace_order.rs` (2) |
| `a_namespace_lowering_page_read_is_held_with_the_writer_gate_free` | A+C | Writer gate of the private index [D3]. D `store_commit.rs::a_held_producer_does_not_hold_workspace_metadata_or_read_capacity`, W `captured_namespace_order.rs` (2) |
| `a_page_read_under_the_writer_gate_reports_the_held_gate` | A+C | Writer gate of the private index [D3]. D `store_commit.rs::a_held_producer_does_not_hold_workspace_metadata_or_read_capacity`, W `captured_namespace_order.rs` (2) |
| **`tests/commit_progress.rs`** (1) | | |
| `a_frozen_transfer_admits_a_mounted_mutation_between_its_frames` | A | D `mounted_install.rs::writes_during_a_commit_stay_live_and_reach_the_next_commit` |
| **`tests/commit_staged.rs`** (14) | | |
| `commit_repeated` | A | D `mounted_commit.rs::r5_8_five_incremental_commits_on_one_mount_each_read_back_completely` |
| `commit_successor` | A | D `store_commit.rs::later_mutation_survives_capture_publication_and_known_install` |
| `commit_mounted_successor` | A | D `mounted_install.rs::writes_during_a_commit_stay_live_and_reach_the_next_commit` |
| `commit_mounted_build_overlap` | A | D `mounted_concurrency.rs::r6_2_a_commit_against_twenty_writers_holds_an_exact_prefix_of_each` |
| `commit_selectors` | C | [D7] (stage selectors) |
| `commit_denied` | C | [D1] (Service denial) |
| `commit_lost_result` | A | D `mounted_commit_failures.rs::f3_unknown_history_outcome_keeps_custody_and_the_mount_still_serves` |
| `commit_reconcile_failure` | A | D `mounted_commit_failures.rs::f4_known_publication_with_an_unattempted_install_keeps_custody` |
| `commit_consumed_stage` | C | [D7] |
| `commit_head_moved` | C | [D7]. Replacement: D `product_commit.rs::a_stale_unchanged_candidate_overwrites_a_moved_head` |
| `commit_cycles` | A | D `mounted_cycles.rs::counts_plateau_over_twelve_cycles_with_maintenance_awaited_after_each_unmount` |
| `commit_headroom` | C | [D3] [D12] |
| `commit_frontier` | A | D `mounted_commit.rs::r5_3_commit_captures_what_every_launch_published_whatever_became_of_the_process` |
| `commit_remote_admission` | A+C | Service admission [D1]. D `mounted_commit_failures.rs::f1_store_busy_before_save_is_settled_and_a_later_commit_is_exact` |
| **`tests/composite.rs`** (15) | | |
| `composite_progress_custody` | C | [D2] [D9] (progress frames of the host command) |
| `composite_clean` | A | D `product_commit.rs::an_unchanged_workspace_is_up_to_date_and_creates_no_commit` |
| `composite_clean_successor` | A | D `mounted_commit.rs::r5_2_unchanged_is_up_to_date_and_a_change_is_committed_once` |
| `composite_repeated` | A | D `product_commit.rs::repeated_incremental_commits_keep_the_store_open_and_each_root_is_exact` |
| `composite_successor` | A | D `store_commit.rs::later_mutation_survives_capture_publication_and_known_install` |
| `composite_native_save` | A | D `product_commit.rs::every_kind_of_change_commits_through_the_product_constructor` |
| `composite_unknown_save` | A | D `store_commit.rs::nested_storage_unknown_keeps_original_capture_and_blocks_another_commit` |
| `composite_lost_result` | A | D `product_commit.rs::an_unknown_history_outcome_keeps_the_reader_and_owner_with_the_failure` |
| `composite_reconcile_failure` | A | D `store_commit.rs::known_publication_survives_an_unattempted_local_install` |
| `composite_denied` | C | [D1] |
| `composite_head_moved` | C | [D7] |
| `composite_refusals` | A | D `product_commit.rs::the_product_constructor_classifies_and_settles_a_definite_refusal` |
| `composite_metadata_only` | A | W `captured_namespace_states.rs::a_base_file_with_only_metadata_changes_keeps_its_content_root` |
| `composite_frontier` | A | D `native_control.rs::product_control_commit_captures_the_frontier_releases_its_owners_and_is_then_up_to_date` |
| `composite_remote_admission` | A+C | [D1]. D `product_commit.rs::a_commit_waits_for_owner_admission_and_is_not_refused` |
| **`tests/create.rs`** (7) | | |
| `create_semantics` | A | W `namespace.rs::create_mkdir_symlink_and_link_publish_parent_and_reference_effects_atomically` |
| `create_flags_permissions` | A | D `native_mutation.rs::namespace_mutations_are_visible_at_once_and_refusals_stay_refused` |
| `create_successor` | A | D `captured_commit.rs::mutations_after_the_capture_stay_out_of_its_root_and_survive_install` |
| `create_capacity` | C | [D3] [D8] (charged identity budget) |
| `create_refusals` | A | W `namespace.rs::unrepresentable_names_targets_and_reservations_are_refused_before_any_job` |
| `create_reserve_denied` | A+C | Service denial [D1]. D `mounted_failure_scope.rs::a_create_with_no_serial_range_under_a_held_store_writer_is_eagain_with_no_effect` |
| `create_reserve_unknown` | A+C | [D1]. same |
| **`tests/fresh_stream.rs`** (2) | | |
| `fresh_stream_mounted_dsh` | A | W `captured_namespace_states.rs::a_fresh_file_is_constructed_from_its_captured_runs` |
| `fresh_stream_captured_replay` | A+C | Replay bound of the host upload [D9]. W `captured_file_edits.rs::length_growth_and_true_new_file_do_not_use_a_fabricated_base` |
| **`tests/keyed_tree.rs`** (1) | | |
| `one_walk_reaches_each_leaf_once_and_one_delete_rewrites_one_path` | C | [D3] (private keyed B+ tree) |
| **`tests/maintenance.rs`** (6) | | |
| `maintenance_payload_churn` | A+C | Private-backing maintenance [D3]. D `mounted_cycles.rs::counts_plateau_over_twelve_cycles_beside_a_running_sibling_writer`, D `owner.rs::idle_cleanup_runs_without_an_explicit_reclaim_or_status_job`, O `cleanup_observation.rs` (2) |
| `maintenance_mutation_churn` | A+C | Private-backing maintenance [D3]. D `mounted_cycles.rs::counts_plateau_over_twelve_cycles_beside_a_running_sibling_writer`, D `owner.rs::idle_cleanup_runs_without_an_explicit_reclaim_or_status_job`, O `cleanup_observation.rs` (2) |
| `maintenance_cross_workspace` | A+C | Private-backing maintenance [D3]. D `mounted_cycles.rs::counts_plateau_over_twelve_cycles_beside_a_running_sibling_writer`, D `owner.rs::idle_cleanup_runs_without_an_explicit_reclaim_or_status_job`, O `cleanup_observation.rs` (2) |
| `maintenance_frozen` | A+C | Private-backing maintenance [D3]. D `mounted_cycles.rs::counts_plateau_over_twelve_cycles_beside_a_running_sibling_writer`, D `owner.rs::idle_cleanup_runs_without_an_explicit_reclaim_or_status_job`, O `cleanup_observation.rs` (2) |
| `maintenance_payload_failure` | A+C | Private-backing maintenance [D3]. D `mounted_cycles.rs::counts_plateau_over_twelve_cycles_beside_a_running_sibling_writer`, D `owner.rs::idle_cleanup_runs_without_an_explicit_reclaim_or_status_job`, O `cleanup_observation.rs` (2) |
| `maintenance_metadata_failure` | A+C | Private-backing maintenance [D3]. D `mounted_cycles.rs::counts_plateau_over_twelve_cycles_beside_a_running_sibling_writer`, D `owner.rs::idle_cleanup_runs_without_an_explicit_reclaim_or_status_job`, O `cleanup_observation.rs` (2) |
| **`tests/mkdir.rs`** (6) | | |
| `mkdir_semantics` | A | W `namespace.rs::create_mkdir_symlink_and_link_publish_parent_and_reference_effects_atomically` |
| `mkdir_successor` | A | W `captured_namespace.rs::mutations_after_the_capture_are_absent_from_its_root_and_present_in_the_next` |
| `mkdir_capacity` | C | [D3] [D8] |
| `mkdir_refusals` | A | W `namespace.rs::unrepresentable_names_targets_and_reservations_are_refused_before_any_job` |
| `mkdir_reserve_denied` | A+C | [D1]. D `mounted_failure_scope.rs::a_create_with_no_serial_range_under_a_held_store_writer_is_eagain_with_no_effect` |
| `mkdir_reserve_unknown` | A+C | [D1]. same |
| **`tests/mounted_create.rs`** (6) | | |
| `mounted_create_kernel` | A | D `native_mutation.rs::write_append_and_truncate_are_exact_in_cache_and_daemon` |
| `mounted_create_existing` | A | D `native_mutation.rs::namespace_mutations_are_visible_at_once_and_refusals_stay_refused` (EEXIST) |
| `mounted_create_visibility` | A | D `native_coherence.rs::a_lookup_racing_create_unlink_and_rename_never_binds_a_stale_inode` |
| `mounted_create_permit` | C | [D5] (reply permit) |
| `mounted_create_notification_failure` | C | [D4] |
| `mounted_create_successor` | A | D `mounted_install.rs::writes_during_a_commit_stay_live_and_reach_the_next_commit` |
| **`tests/mounted_mkdir.rs`** (4) | | |
| `mounted_mkdir_kernel` | A | D `native_mutation.rs::namespace_mutations_are_visible_at_once_and_refusals_stay_refused` |
| `mounted_mkdir_visibility` | A | same |
| `mounted_mkdir_permit` | C | [D5] |
| `mounted_mkdir_notification_failure` | C | [D4] |
| **`tests/mounted_namespace.rs`** (2) | | |
| `mounted_namespace_kernel` | A | D `native_mutation.rs::namespace_mutations_are_visible_at_once_and_refusals_stay_refused`, D `mounted_links.rs` |
| `mounted_namespace_durability` | A | D `mounted_commit.rs::r5_1_external_bash_changes_of_every_kind_are_committed_and_read_back_on_a_fresh_mount` |
| **`tests/mounted_symlink.rs`** (4) | | |
| `mounted_symlink_kernel` | A | D `native_mutation.rs::namespace_mutations_are_visible_at_once_and_refusals_stay_refused`, D `read_cost.rs::read_open_and_readlink_cost_exactly_this_at_any_size_and_beside_unrelated_rows` |
| `mounted_symlink_visibility_successor` | A | D `mounted_commit.rs::r5_1_external_bash_changes_of_every_kind_are_committed_and_read_back_on_a_fresh_mount` |
| `mounted_symlink_permit` | C | [D5] |
| `mounted_symlink_notification_failure` | C | [D4] |
| **`tests/namespace.rs`** (10) | | |
| `namespace_setattr` | A | W `namespace.rs::chmod_and_utimens_follow_the_portable_grammar_and_keep_one_mtime` |
| `namespace_mknod` | A | W `namespace.rs::create_mkdir_symlink_and_link_publish_parent_and_reference_effects_atomically` |
| `namespace_link` | A | same; W `directory_links.rs` (5) |
| `namespace_unlink` | A | W `namespace.rs::unlink_and_rmdir_remove_one_reference_and_keep_directory_counts_exact` |
| `namespace_unlink_fresh` | A | W `captured_namespace_states.rs::inodes_created_and_removed_inside_the_capture_are_never_constructed` |
| `namespace_rmdir` | A | W `namespace.rs::unlink_and_rmdir_remove_one_reference_and_keep_directory_counts_exact` |
| `namespace_rename` | A | W `namespace.rs::rename_moves_replaces_and_refuses_cycles_without_descendant_rows` |
| `namespace_rename_base` | A | W `captured_namespace_states.rs::a_moved_base_directory_has_no_row_and_no_value` |
| `namespace_generation` | A | W `namespace_view.rs::known_install_folds_a_capture_and_keeps_later_namespace_changes` |
| `namespace_notification_failure` | C | [D4] |
| **`tests/open.rs`** (8) | | |
| `open_permissions` | A | W `native_visit.rs::a_visit_through_a_read_only_descriptor_is_refused_and_changes_nothing`, D `native_mutation.rs` (`refusals`) |
| `open_modes` | A | W `native_visit.rs::an_open_visit_decides_a_regular_file_and_records_its_descriptor_alone` |
| `open_capacity` | C | [D8] (128 handles) |
| `open_last_slot` | C | [D8] |
| `open_metadata_failure` | C | [D3] |
| `open_forget` | A | D `native_custody.rs::kernel_forget_arrives_on_a_live_connection_with_its_exact_decrement`, D `mounted_drain.rs::fp31_batched_forget_is_exact_and_outstanding_lookups_retire_in_bounded_turns_after_detach` |
| `open_deadline` | C | [D5] |
| `open_successor` | A | D `mounted_install.rs::retained_descriptors_and_kernel_caches_are_unchanged_by_install` |
| **`tests/payload.rs`** (2) | | |
| `private_payload_profile_explicitly_requires_linux` | C | [D3] |
| `linux_owned_payload_contract` | A+C | [D3]. O `payload.rs` (4), W `payload.rs` (4) |
| **`tests/pieces_sequence.rs`** (34) | | |
| all 34 tests | C | [D3] (page format, splice and cursor of the private extent sequence). File content semantics: W `payload.rs` (4), W `captured_namespace_fragments.rs` (9), O `payload_fragmentation.rs`, O `payload_runs.rs` (4) |
| **`tests/prepared_namespace.rs`** (3) | | |
| `a_wide_directory_lowers_to_its_exact_rows_and_one_head` | A+C | Prepared stream [D9]. W `captured_namespace_cursor.rs` (3), D `mounted_install.rs::a_directory_of_more_than_one_name_window_is_listed` |
| `a_wide_name_pass_over_one_identity_reads_each_path_once` | A+C | Prepared stream [D9]. W `captured_namespace_cursor.rs` (3), D `mounted_install.rs::a_directory_of_more_than_one_name_window_is_listed` |
| `a_prepared_update_past_the_metadata_frame_travels_as_a_stream` | A+C | Prepared stream [D9]. W `captured_namespace_cursor.rs` (3), D `mounted_install.rs::a_directory_of_more_than_one_name_window_is_listed` |
| **`tests/readable.rs`** (10) | | |
| `immutable_reads_handles_eof_and_terminal_failure` | A | W `base.rs::full_root_binding_aliases_metadata_symlinks_and_eof_use_content_apis` |
| `directory_cookies_are_stable_and_bound_to_handles` | A | W `native_directory.rs::empty_whiteout_pages_continue_and_only_accepted_names_become_resume_cookies`, D `mounted_rewind.rs` |
| `mount_lease_reply_ownership_admission_and_clean_close` | A+C | Separate close [D6]. D `native_mount.rs::ready_mount_serves_the_complete_root_to_unregistered_access_then_drains` |
| `branch_symlink_permission_and_failed_attach_are_explicit` | A | D `native_mount.rs::attach_refusals_have_no_effect_and_a_failed_attach_returns_to_unattached` |
| `full_cookie_table_still_replays_existing_positions` | A+C | Fixed cookie table [D8]. D `directory_cost.rs::a_rewound_handle_stores_one_listing_and_refuses_the_offsets_it_gave_before` |
| `root_owner_reads_mode_zero_but_execute_requires_an_execute_bit` | B | Execute permission on a mounted file. **Migrated**: D `mounted_execute.rs::a_stored_program_executes_and_its_in_place_rewrite_executes_next` (added in this stage; Linux receipt `execute-attempt1-linux-mounted_execute.txt`) |
| `detach_waits_for_projection_handles_and_preserves_local_handles` | A | D `mounted_drain.rs::fp21_a_parked_release_keeps_unmount_draining_until_its_credits_return_inside_the_window`, D `forced_unmount.rs::fp33_a_held_descriptor_leaves_the_mount_aborted_and_retained` |
| `registry_grows_on_demand_and_keeps_retained_capacity_charged` | C | [D3] [D8] |
| `failed_close_keeps_its_registry_count_until_cleanup_succeeds` | A+C | [D6]. D `native_custody.rs::an_unmount_that_cannot_revoke_stops_retained_and_later_replies_repeat_it` |
| `readonly_component_chain_retains_all_270_forgotten_ancestors` | A | W `native_directory.rs::opened_directory_retains_parent_and_metadata_after_forget_and_rmdir`, O `native_lookup.rs` (10) |
| **`tests/resize.rs`** (9) | | |
| `resize_semantics` | A | W `payload.rs::truncate_and_regrow_never_resurrect_bytes_across_capture_and_known_install` |
| `resize_envelope` | C | [D8] (8 MiB zero-extension envelope) |
| `resize_successor` | A | W `captured_namespace_fragments.rs::shrinking_and_regrowing_across_cells_never_returns_cut_bytes` |
| `resize_overwritten_zero` | A | W `captured_namespace_fragments.rs::a_hole_and_written_zeros_are_the_same_bytes` |
| `resize_metadata_only` | A | W `captured_namespace_states.rs::a_base_file_with_only_metadata_changes_keeps_its_content_root` |
| `resize_refusals` | A | W `native_visit.rs::a_visit_through_a_read_only_descriptor_is_refused_and_changes_nothing` |
| `resize_metadata_failure` | C | [D3] |
| `resize_native_save` | A | D `captured_commit.rs::every_kind_of_change_commits_through_the_driver_and_reads_back_from_a_fresh_bind` |
| `resize_frontier` | A | W `captured_namespace_fragments.rs::a_truncate_between_pieces_cuts_what_was_written_before_it` |
| **`tests/stage.rs`** (10) | | |
| `stage_semantics` | A+C | Separate staging [D7]. D `store_commit.rs::committed_up_to_date_overwrite_and_new_bind_preserve_exact_roots` |
| `stage_native_save` | A | D `captured_commit.rs::every_kind_of_change_commits_through_the_driver_and_reads_back_from_a_fresh_bind` |
| `stage_unknown_save` | A | D `store_commit.rs::nested_storage_unknown_keeps_original_capture_and_blocks_another_commit` |
| `stage_metadata_denied` | C | [D1] |
| `stage_lowering` | A+C | [D9]. W `captured_namespace.rs` (9) |
| `stage_head_moved` | C | [D7] |
| `stage_frontier` | A | D `captured_commit.rs::mutations_after_the_capture_stay_out_of_its_root_and_survive_install` |
| `stage_completion_failure` | A | D `captured_commit.rs::a_failed_attempt_keeps_its_custody_publishes_nothing_and_a_later_commit_is_exact` |
| `stage_metadata_only` | A | W `captured_namespace_states.rs::link_count_changes_alone_keep_content_and_metadata_roots` |
| `stage_headroom` | C | [D3] [D12] |
| **`tests/symlink.rs`** (7) | | |
| `symlink_semantics` | A | W `namespace.rs::create_mkdir_symlink_and_link_publish_parent_and_reference_effects_atomically`, W `nonfile.rs` (3) |
| `symlink_successor` | A | W `captured_namespace_states.rs::a_fresh_symlink_takes_exactly_its_captured_target` |
| `symlink_capacity` | C | [D3] [D8] |
| `symlink_refusals` | A | W `namespace.rs::unrepresentable_names_targets_and_reservations_are_refused_before_any_job` |
| `symlink_backing_failure` | C | [D3] |
| `symlink_reserve_denied` | A+C | [D1]. D `mounted_failure_scope.rs::a_create_with_no_serial_range_under_a_held_store_writer_is_eagain_with_no_effect` |
| `symlink_reserve_unknown` | A+C | [D1]. same |
| **`tests/wide_namespace.rs`** (5) | | |
| `one_directory_accepts_many_names_and_survives_rename_and_removal` | A | W `namespace_view.rs::name_and_inode_counts_have_no_cap_and_every_window_stays_bounded` |
| `one_unlink_of_a_wide_directory_rewrites_one_path` | A+C | Tree path rewrite [D3]. W `native_unlink_cost.rs::one_removed_file_costs_exactly_this_beside_any_files_and_after_any_orphans` |
| `a_generation_admits_many_identities_while_its_budget_has_room` | C | [D3] [D8] |
| `create_cost_by_64_public_calls` | A | W `native_visit_cost.rs::the_five_jobs_of_one_created_file_cost_exactly_this_at_any_directory_size` |
| `a_spent_budget_refuses_precisely_and_publishes_nothing` | C | [D3] [D8] |
| **`tests/write.rs`** (12) | | |
| `write_positional` | A | W `payload.rs::writes_change_only_their_window_and_never_read_the_inherited_payload` |
| `write_append` | A | W `namespace_daemon.rs::concurrent_appenders_and_a_tail_reader_see_whole_records_through_real_owner_jobs` |
| `write_zero_rights` | A | W `native_visit.rs::a_visit_through_a_read_only_descriptor_is_refused_and_changes_nothing` |
| `write_envelope` | C | [D8] (per-write envelope cap) |
| `write_frontier` | A | W `captured_namespace_fragments.rs::pieces_crossing_the_end_of_a_base_file_give_one_root` |
| `write_quota` | A+C | [D12]. O `device_capacity.rs`, D `device_capacity.rs` |
| `write_metadata_failure` | C | [D3] |
| `write_released` | A | W `captured_namespace_holds.rs::a_file_created_held_written_unlinked_and_written_again_is_never_constructed`, W `namespace_daemon_cases/orphans.rs` (2) |
| `write_deadline` | C | [D5] |
| `write_pending` | C | [D5] (reply permit) |
| `write_successor` | A | D `store_commit.rs::later_mutation_survives_capture_publication_and_known_install` |
| `write_native_save` | A | D `captured_commit.rs::every_kind_of_change_commits_through_the_driver_and_reads_back_from_a_fresh_bind` |

Tests by class: A 88, A+C 38, B 2, C 76.

## Other tracked files

| File | Class | Note |
| --- | --- | --- |
| `Cargo.toml` | C | Depends on the active `layerfs-bridge` and `layerfs-fuse`, whose APIs it does not match; cannot be loaded |
| `tests/*.py (24 files)` | C | Docker route drivers and fixture preparation for the tests above; none is referenced outside this crate |
| `tests/support/native_workspace.rs` | C | Fixture over the host Service [D1] |
| `tests/support/prepared.rs` | C | Prepared-stream helpers [D9] |
| `tests/store_lock.c` | C | Observer of the host Store lock for the Docker drivers [D1] |
