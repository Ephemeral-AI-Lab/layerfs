"""Coverage classification of the three predecessors with no open owner question.

Each entry: path-or-test -> (class, behaviour, where covered / which direction).
Short test-directory names: D layerfs-daemon/tests, W layerfs-workspace/tests,
O layerfs-overlay/tests, F layerfs-fuse/tests, B layerfs-bridge/tests,
S layerfs-sandbox/tests, K layerfs-api/sdk/tests, P layerfs-project/tests,
C layerfs-content/tests, PE layerfs-persistence/tests, ST layerfs-storage/tests.
"""

SERVER_SRC = {
    "src/bin/layerfs-server.rs": ("C", "Operator process entry of the host Server", "[D1]"),
    "src/lib.rs": ("C", "Exports of Service and Server", "[D1]"),
    "src/host/mod.rs": ("C", "Declarations of the host assembly", "[D1]"),
    "src/host/acceptor.rs": ("C", "Bounded native connection acceptor of the host Server", "[D1]. The daemon's own control acceptor is `layerfs-daemon/src/application/serve.rs` (D `native_application.rs`)"),
    "src/host/assembly.rs": ("C", "One host authority: Store, history, Service, listener and Sandbox owner", "[D1]"),
    "src/host/config.rs": ("C", "Telemetry and history-catalog configuration of the Server", "[D1]. Daemon configuration is `layerfs-daemon/src/application/config.rs`"),
    "src/host/run.rs": ("C", "Operator assembly: configured keys, Store path, one acceptor", "[D1]"),
    "src/host/store.rs": ("A+C", "Store, history, grant and capacity construction", "Host route [D1]. Store create/open: SDK `project/init.rs` (K `init.rs::init_seals_complete_root_and_first_branch_without_retained_handles`), daemon `bootstrap.rs` (D `native_install.rs::authenticated_install_opens_one_store_and_reads_the_complete_root`)"),
    "src/service/mod.rs": ("C", "Declarations of the authorized Service", "[D1]"),
    "src/service/error.rs": ("A+C", "Keep C1/C2 failure distinctions across the Service", "Service wire [D1]. Direct Store failures keep their original cause: D `store_commit.rs::busy_missing_dependency_and_read_failure_leave_no_stage_or_local_data_loss`"),
    "src/service/handler.rs": ("A+C", "Store authority, grants, per-Store writer admission, read bound", "Grants and writer budgets [D1]. Replacement rule is one before-effect Busy and fair readers [D5]: PE `serverless_store.rs::second_process_busy_has_no_effect_and_readers_keep_progressing`, D `installed_store.rs::direct_reads_finish_under_another_process_writer_and_busy_stays_typed`, D `store_read_service.rs` (10 tests)"),
    "src/service/init_project.rs": ("A", "Project Init through the Service", "303/07 §4.1: Init belongs in `layerfs-project`. P `init_sqlite.rs::selected_profiles_init_100_and_1000_pass_the_full_namespace_oracle`, K `init.rs` (2 tests)"),
    "src/service/input.rs": ("C", "Sequential wire input, incomplete until exact EOF", "[D1] [D9]"),
    "src/service/read/mod.rs": ("C", "Declarations", "[D1]"),
    "src/service/read/catalog.rs": ("A+C", "Read-only history requests", "Service route [D1]. History reads: PE `history_lifecycle.rs::history_reads_return_the_recorded_ancestry_and_chain`, PE `history_history_pages.rs` (3 tests); control verbs fork/history: D `native_control.rs::native_mount_commit_status_fork_history_and_terminal_unmount`"),
    "src/service/read/content.rs": ("A+C", "Bounded content reads and inspection variants", "Service route [D1]. Daemon reads the Store directly: W `base.rs::full_root_binding_aliases_metadata_symlinks_and_eof_use_content_apis`, D `installed_store.rs` (3 tests), D `mounted_store_read.rs` (4 tests)"),
    "src/service/records.rs": ("C", "Service wire record conversion", "[D1] [D9]"),
    "src/service/save/mod.rs": ("C", "Declarations", "[D1]"),
    "src/service/save/catalog.rs": ("A+C", "Stage, commit, fork and reservation commands after admission", "Service route [D1]. PE `history_conditional_updates.rs` (6 tests), PE `atomic_publication.rs` (3 tests), PE `history_allocation.rs` (3 tests); Commit publication: D `store_commit.rs` (8 tests)"),
    "src/service/save/content.rs": ("A+C", "One Storage Save per mutation; root delivered after EOF and finish", "Service route [D1]. Save lifecycle: PE `interleaved_saves.rs`, PE `sqlite_transaction_units.rs` (10 tests); `Save::finish` in Commit: D `store_commit.rs::committed_up_to_date_overwrite_and_new_bind_preserve_exact_roots`"),
    "src/service/save/file_stream.rs": ("A+C", "Host-side replay of one final Base/Local/Zero file sequence", "Host-side construction [D1] [D9]. Captured files are normalized in the daemon: W `captured_file_edits.rs` (14 tests), W `captured_namespace_fragments.rs` (9 tests)"),
    "src/service/save/file_stream/origin_runs.rs": ("A+C", "Navigation of replacements resolved from the base", "As `file_stream.rs`. W `captured_file_edits.rs::inherited_gap_and_window_crossing_authenticated_eof_are_zero_without_base_fallback`"),
    "src/service/save/filesystem.rs": ("A+C", "Inode values, directory metadata and fresh declarations under one Save", "Host-side construction [D9]. `CapturedNamespace`: W `captured_namespace.rs` (9 tests), W `captured_namespace_states.rs` (18 tests)"),
    "src/service/save/import/mod.rs": ("A", "Declarations of the import route", "see below"),
    "src/service/save/import/batch/mod.rs": ("A", "Declarations", "see below"),
    "src/service/save/import/batch/producer.rs": ("A", "Ordered messages from file constructors to one Save owner", "`layerfs-project` import with bounded batches into `Save::accept`: P `backed_acquisition.rs` (2 tests), P `namespace_scaling.rs`"),
    "src/service/save/import/namespace.rs": ("A", "Namespace initialization and inode role validation", "P `complete_root.rs` (2 tests), P `init_sqlite.rs` (4 tests), P `init_memory.rs`"),
    "src/service/save/import/scan.rs": ("A", "One directory import into a single Save", "P `complete_root.rs::native_import_keeps_ignored_git_dependencies_outputs_and_exact_link_targets`, P `acquisition_custody.rs` (6 tests)"),
    "src/service/save/metadata.rs": ("A+C", "Fresh portable attribute tree or a patch of an existing one", "Service constructor route [D9]. W `captured_namespace_metadata.rs` (2 tests), C `filesystem_attributes.rs`"),
    "src/service/save/prepared.rs": ("C", "Prepared namespace stream received into a charged row spool", "[D9]; its 256 MiB total-upload restriction is a named retirement target [D8]"),
    "src/service/save/validation.rs": ("A", "Published roots carry the declared inode roles", "Content validation on the backed route: W `captured_namespace_perturbed.rs` (4 tests), PE `semantic_admission.rs` (2 tests)"),
}

SERVER_TESTS = {
    "tests/acceptor_idle.rs": {
        "default_session_limit_includes_two_readers": ("C", "[D1] (host acceptor)"),
        "flag_acceptor_ignores_closed_stdin": ("C", "[D1] (host acceptor)"),
    },
    "tests/admission.rs": {
        "four_writers_are_admitted_then_refused_and_reads_never_queue_behind_them": ("A+C", "Writer budget [D1]. Replacement: PE `serverless_store.rs::second_process_busy_has_no_effect_and_readers_keep_progressing`"),
        "every_supported_budget_admits_its_writers_and_refuses_the_next": ("C", "[D1] (configured writer budgets do not exist; one writer, one Busy [D5])"),
        "two_sandboxes_over_one_store_share_its_budget": ("A+C", "[D1]. Several daemons over one Store: D `shared_processes.rs::overlapping_saves_ordered_overwrites_and_killed_daemon_preserve_the_shared_store` (known failing for want of a precondition; not a regression)"),
        "another_store_keeps_its_own_budget": ("C", "[D1]"),
        "a_busy_reader_bound_neither_blocks_writers_nor_exceeds_itself": ("A+C", "[D1]. D `store_read_service.rs::idle_selection_and_replaced_wakeup_do_not_wait_on_a_busy_reader`, D `store_commit.rs::a_held_producer_does_not_hold_workspace_metadata_or_read_capacity`"),
    },
    "tests/construct_metadata.rs": {
        "constructor_without_history_saves_portable_trees_and_reuses_exact_roots": ("A", "W `captured_namespace_metadata.rs::a_metadata_patch_keeps_every_other_key_and_its_value_root`, W `captured_namespace_states.rs::a_base_file_with_only_metadata_changes_keeps_its_content_root`"),
        "validation_authorization_empty_input_and_deadline_fail_before_save": ("C", "[D1] grants, [D5] deadline. Portable-grammar refusal: W `namespace.rs::chmod_and_utimens_follow_the_portable_grammar_and_keep_one_mtime`"),
        "real_store_admission_failure_is_definite_and_explicit_abort_releases_the_slot": ("A+C", "[D1]. D `mounted_commit_failures.rs::f1_store_busy_before_save_is_settled_and_a_later_commit_is_exact`"),
    },
    "tests/construct_symlink.rs": {
        "constructor_without_history_preserves_exact_targets_and_canonical_reuse": ("A", "W `captured_namespace_states.rs::a_fresh_symlink_takes_exactly_its_captured_target`, `a_base_directory_and_symlink_keep_their_content_roots_under_new_metadata`"),
        "target_authority_body_and_deadline_refusals_do_not_begin_a_save": ("C", "[D1] [D5]. Target grammar: W `namespace.rs::unrepresentable_names_targets_and_reservations_are_refused_before_any_job`"),
        "occupied_save_refuses_definitely_and_explicit_owner_abort_restores_admission": ("A+C", "[D1]. D `mounted_commit_failures.rs::f5_store_busy_inside_the_producer_is_settled_and_releases_both_owners`"),
    },
    "tests/direct.rs": {
        "construct_read_edit_inspect_and_reopen": ("A", "C `file_complete.rs`, C `file_read.rs`, C `edit_single.rs`; reopen: PE `sqlite_publication.rs::canonical_save_read_reuse_and_persisted_reservations_survive_reopen`"),
        "ten_mib_final_replacement_saves_and_keeps_the_previous_root": ("A", "W `captured_file_edits.rs::overlapping_writes_shrink_regrow_and_cross_cell_spans_match_final_bytes`; previous root kept: D `mounted_install.rs::a_later_commit_overwrites_the_branch_head_and_keeps_its_captured_parent`"),
        "final_extents_match_the_sealed_reference_root_and_partition": ("A+C", "Host final-extent stream [D9]. W `captured_namespace_fragments.rs::fragmented_writes_of_a_base_file_give_one_root`"),
        "final_file_stream_handles_zero_delete_noop_and_malformed_input": ("A+C", "[D9]. W `captured_namespace_fragments.rs::a_hole_and_written_zeros_are_the_same_bytes`, W `captured_file_edits.rs::same_byte_overwrite_retains_original_root_with_one_classification`"),
        "authenticated_generic_save_accepts_4097_separated_final_runs": ("A", "No edit-count cap [D8]: W `captured_file_edits.rs::many_edits_use_numbered_records_after_install_and_reads_survive_close`"),
        "a_spooled_final_stream_replays_many_runs_with_exact_bytes": ("C", "[D9] (the spool is the host-side mechanism)"),
        "denial_partial_and_invalid_inputs_never_succeed": ("C", "[D1] (Service grants and wire input)"),
        "authenticated_network_after_nonblocking_accept": ("A+C", "[D1]. Authenticated channel: B `native.rs::authenticated_duplex_records_stream_beyond_one_window_with_fixed_buffers`"),
        "inherited_deadline_expires_before_direct_input_or_mutation": ("C", "[D5] (no deadline on a Store operation)"),
        "two_writers_overlap_and_capacity_is_reclaimed_after_reverse_completion": ("A", "PE `interleaved_saves.rs::scoped_registry_interleaves_pending_reads_finish_abort_and_slot_reuse`"),
        "versioned_save_resolves_backward_base_without_changing_v1_or_canonical_identity": ("C", "[D9] (v1/v2 wire Save). Canonical identity under install: W `captured_file_edits.rs::prepare_after_actual_binding_install_rebinds_only_the_retained_original_root`"),
    },
    "tests/history.rs": {
        "native_directory_import_exceeds_bootstrap_and_reads_bytes": ("A", "P `complete_root.rs::real_host_profiles_acquire_and_reuse_the_exact_complete_root`"),
        "native_directory_import_spills_ordering_without_an_entry_cap": ("A", "P `backed_acquisition.rs::very_wide_directory_is_ordered_through_many_windows`"),
        "native_import_gives_each_file_its_own_content_root": ("A", "P `backed_acquisition.rs::wide_nested_aliased_root_equals_the_whole_namespace_constructor`"),
        "legacy_mask_grants_no_history": ("C", "[D1] (Service grants)"),
        "history_profile_and_opcode_must_agree": ("C", "[D1] [D9] (wire profile)"),
        "empty_namespace_initializes_and_reads_back": ("A", "PE `history_lifecycle.rs::genesis_is_atomic_and_unique`"),
        "native_import_initialization_builds_a_real_namespace": ("A", "K `init.rs::init_seals_complete_root_and_first_branch_without_retained_handles`"),
        "stage_commit_add_layer_and_read_back": ("A", "PE `history_conditional_updates.rs::exact_stages_publish_in_order_with_their_captured_parents`, PE `history_lifecycle.rs::history_reads_return_the_recorded_ancestry_and_chain`"),
        "stale_loser_retains_its_exact_stage": ("C", "[D7] (overwrite-only; a refused publish leaves no stage). Replacement: PE `atomic_publication.rs::overwrite_and_current_root_up_to_date_each_use_one_transaction`"),
        "multiple_no_change_stages_are_up_to_date": ("A", "PE `history_conditional_updates.rs::a_commit_whose_root_equals_its_base_is_no_changes`, D `product_commit.rs::an_unchanged_workspace_is_up_to_date_and_creates_no_commit`"),
        "delayed_discard_token_cannot_consume_a_replacement_stage": ("A", "PE `history_conditional_updates.rs::an_exact_token_is_the_only_stage_a_discard_removes`"),
        "already_published_source_is_up_to_date_before_stale_head_refusal": ("A", "PE `history_conditional_updates.rs::an_exact_earlier_publication_is_up_to_date_before_a_stale_head`"),
        "wrong_role_root_is_refused": ("A", "PE `history_conditional_updates.rs::stage_insertion_enforces_the_frozen_context`, PE `semantic_admission.rs::omitted_wire_references_cannot_hide_an_inode_dependency_from_save`"),
        "rebasing_by_editing_a_token_is_refused": ("A", "PE `history_conditional_updates.rs::stage_insertion_enforces_the_frozen_context`"),
        "inode_reservations_are_scope_wide_and_never_recycled": ("A", "PE `history_allocation.rs::reservations_are_monotone_half_open_and_per_scope`, PE `history_remediation.rs::concurrent_reservations_never_overlap_and_discard_does_not_refund`"),
        "metadata_only_commands_never_touch_the_content_store": ("C", "[D1] (Service split of content and catalog authority)"),
        "history_pages_are_bounded_and_cursors_are_bound_to_their_range": ("A", "PE `history_history_pages.rs::a_cursor_is_bound_to_its_range_subject_and_content`, `every_bounded_list_paginates_without_gaps_or_repeats`"),
        "read_only_reopen_supports_reads_and_refuses_every_mutation": ("A", "PE `history_reopen.rs::a_read_only_reopen_reads_every_record_and_mutates_nothing`"),
        "encoded_record_widths_match_the_catalog_bounds": ("C", "[D9] (Service wire widths). Current control records: B `control_golden.rs` (5 tests)"),
        "empty_and_interleaved_directories_have_real_canonical_pages": ("A", "C `filesystem_rows.rs`, C `filesystem_read.rs`"),
        "branch_root_descriptor_validates_content_scope_profile_and_actual_serial": ("A", "PE `history_remediation.rs::exact_source_publication_revalidates_immutable_provenance`, PE `history_reopen.rs::an_incompatible_or_foreign_catalog_is_refused`"),
        "composite_failure_keeps_acknowledged_stage_distinct_from_absence": ("A+C", "Retained stage [D7]. Unknown outcome custody: D `store_commit.rs::unknown_history_acknowledgement_retains_original_intent_and_capture`"),
        "refreshed_stack_head_reports_typed_stale_base_on_direct_and_native_routes": ("C", "[D7] (HeadMoved acceptance is historical)"),
        "disjoint_file_edits_still_refuse_stale_publication_without_merging": ("C", "[D7]. Replacement: D `product_commit.rs::a_stale_unchanged_candidate_overwrites_a_moved_head`"),
        "complete_read_attributes_match_over_authenticated_transport": ("A+C", "Service route [D1]. F `attributes.rs::attributes_preserve_signed_fraction_identity_mode_and_link_count`"),
        "daemon_control_is_refused_by_service_even_with_every_store_grant": ("C", "[D1]"),
        "portable_metadata_update_saves_one_tree_and_preserves_all_generic_roots": ("A", "W `captured_namespace_metadata.rs::a_metadata_patch_keeps_every_other_key_and_its_value_root`"),
        "metadata_refusals_preserve_admission_and_never_use_legacy_grants": ("C", "[D1]"),
    },
    "tests/owned_load.rs": {
        "two_native_writers_keep_streaming_heap_bounded_as_input_grows": ("A+C", "Service route [D1]. P `namespace_scaling.rs::backed_counts_grow_while_resident_buffers_stay_fixed`"),
    },
    "examples/verify_namespace.rs": {
        "lite_sample_covers_anchors_empty_sizes_and_tree_tail": ("A", "Same test in `layerfs-project/examples/verify_namespace.rs`"),
    },
}

SERVER_OTHER = {
    "Cargo.toml": ("C", "Depends on `../layerfs-api/core`, removed by F12; cannot be loaded ([receipt 02](02-cargo-cannot-load.txt))"),
    "examples/create_store.rs": ("A", "Store creation: `layerfs-project/examples/benchmark_init.rs`, SDK `ProjectApi::init`"),
    "examples/prepare_store.rs": ("A", "`layerfs-project/examples/benchmark_init.rs`; daemon fixtures in `layerfs-daemon/tests/support/complete_fixture.rs`"),
    "examples/verify_namespace.rs": ("A", "`layerfs-project/examples/verify_namespace.rs`"),
    "examples/verify_checkpoint5.rs": ("C", "Verifier of the withdrawn host-mediated `workspace_write` family [D1]; runnable only at that family's pinned source"),
    "examples/verify_shell.rs": ("C", "Verifier of the withdrawn host-mediated shell-package family [D1] [D2]; runnable only at its pinned source"),
    "tests/construct_metadata_route.py": ("C", "Docker route driver for the Service constructor [D1]"),
    "tests/construct_symlink_route.py": ("C", "Docker route driver for the Service constructor [D1]"),
    "tests/exit_failure.py": ("C", "Server process exit driver [D1]"),
    "tests/interoperability.py": ("C", "Server/daemon interoperability driver [D1]"),
    "tests/shutdown.py": ("C", "Server shutdown driver [D1]"),
    "tests/support/file_save.rs": ("C", "Helper of `direct.rs` and `history.rs`"),
}

FUSE_SRC = {
    "src/lib.rs": ("A", "Exports `mount`, `mount_writable`, `MountHandle`", "`layerfs-fuse/src/lib.rs`; one mutable mount (the read-only profile is not carried, [D13])"),
    "src/adapter.rs": ("A+C", "Kernel callbacks: INIT, LOOKUP, FORGET, GETATTR, ACCESS, OPEN, READ, READLINK, FLUSH, RELEASE, OPENDIR, READDIR, RELEASEDIR, STATFS, FSYNC, SETATTR, WRITE, MKNOD, MKDIR, UNLINK, RMDIR, SYMLINK, RENAME, LINK, CREATE; xattr and READDIRPLUS refused", "`layerfs-fuse/src/request/callbacks.rs` serves the same set and declares every other opcode in `operations/unsupported.rs` with a count (`request/accounting.rs`). Read path: D `native_mount.rs` (3), D `read_cost.rs` (5), D `directory_cost.rs` (4); mutation: D `native_mutation.rs` (2), D `native_coherence.rs` (6). Dropped with the mechanism: direct-I/O profile and per-WRITE invalidation [D4], the 128-handle table [D8], blocking reply permits [D5]"),
    "src/mount.rs": ("A+C", "Session ownership, mount/unmount and bounded callback drain", "`layerfs-fuse/src/session/` and `mount/`: D `native_mount.rs::attach_refusals_have_no_effect_and_a_failed_attach_returns_to_unattached`, D `mounted_drain.rs` (5), D `forced_unmount.rs` (6), F `fence.rs` (4). Startup deadlines are not carried [D5]"),
    "src/replies.rs": ("A", "Workspace results to kernel replies", "`layerfs-fuse/src/request/reply.rs`, `attributes.rs`: F `attributes.rs` (3 tests)"),
    "src/trace.rs": ("A", "Env-gated per-callback diagnostics", "Opcode accounting in the status receipt: `layerfs-fuse/src/request/accounting.rs`, read in D `native_coherence.rs` and D `mounted_drain.rs`"),
}

FUSE_TESTS = {
    "tests/kernel_write.rs": {
        "kernel_write_positional": ("A", "D `native_mutation.rs::write_append_and_truncate_are_exact_in_cache_and_daemon`; incremental Commits: D `mounted_commit.rs::r5_8_five_incremental_commits_on_one_mount_each_read_back_completely`"),
        "kernel_write_append": ("A", "D `native_coherence.rs::size_never_shrinks_under_concurrent_append_and_refusals_stay_refused`"),
        "kernel_write_read_race": ("A+C", "The old gate refused a WRITE while a READ was held; that refusal is withdrawn [D5]. Required behaviour now: D `mounted_parking.rs::fp8_a_sibling_write_and_a_same_mount_write_complete_while_cold_reads_are_parked`"),
        "kernel_write_mappings": ("A+C", "Direct-I/O profile refusing shared mappings [D4]. Cached profile: D `native_coherence.rs::shared_mapping_stores_are_published_and_never_change_size`, D `mounted_commit.rs::r5_9_stores_through_shared_mappings_are_committed_once_written_back`"),
        "kernel_write_exec": ("B", "A program stored in the Workspace executes, and executes as its replacement after an in-place rewrite. No active test executed anything from a mount. **Migrated**: D `mounted_execute.rs::a_stored_program_executes_and_its_in_place_rewrite_executes_next` (added in this stage; Linux receipt `execute-attempt1-linux-mounted_execute.txt`)"),
        "kernel_write_native_save": ("A", "D `mounted_install.rs::writes_during_a_commit_stay_live_and_reach_the_next_commit`, D `mounted_concurrency.rs::r6_2_a_commit_against_twenty_writers_holds_an_exact_prefix_of_each`"),
        "kernel_write_ingress": ("A", "W `payload.rs::writes_change_only_their_window_and_never_read_the_inherited_payload`"),
        "kernel_write_quota": ("A+C", "Per-Workspace private-disk budget [D12]. Device-full behaviour: D `device_capacity.rs::real_owner_reclaims_at_device_full_after_last_owner_without_a_cleanup_job`, O `device_capacity.rs`"),
        "kernel_write_backing_failure": ("C", "[D3] (failure of the private backing allocator)"),
        "kernel_write_origin": ("C", "[D3] [D5] (projection reply-slot API of the old Workspace, no mount)"),
        "kernel_write_completion_failure": ("C", "[D4] (failure of the per-WRITE notifier)"),
    },
    "tests/kernel_resize.rs": {
        "kernel_resize_semantics": ("A", "D `native_mutation.rs::write_append_and_truncate_are_exact_in_cache_and_daemon`, W `payload.rs::truncate_and_regrow_never_resurrect_bytes_across_capture_and_known_install`"),
        "kernel_resize_open_trunc": ("A", "D `native_mutation.rs::write_append_and_truncate_are_exact_in_cache_and_daemon` (O_TRUNC of a cached inherited file)"),
        "kernel_resize_same_length": ("A", "W `captured_namespace_states.rs::a_base_file_with_only_metadata_changes_keeps_its_content_root`"),
        "kernel_resize_refused": ("A", "D `native_mutation.rs::namespace_mutations_are_visible_at_once_and_refusals_stay_refused`, W `native_visit.rs::a_visit_through_a_read_only_descriptor_is_refused_and_changes_nothing`"),
        "kernel_resize_failed_open": ("C", "[D12] (quota-refused SETATTR after OPEN)"),
        "kernel_resize_metadata_failure": ("C", "[D3] (private metadata allocation failure)"),
        "kernel_resize_envelope": ("A+C", "The 8 MiB zero-extension envelope is an artificial cap [D8]. Sparse extension: D `complete_installed_roots.rs::sparse_native_bytes_and_holes_survive_install`, W `captured_file_edits.rs::new_hole_above_four_gib_uses_bounded_original_run_work`"),
        "kernel_resize_native_save": ("A", "D `mounted_install.rs::writes_during_a_commit_stay_live_and_reach_the_next_commit`"),
        "kernel_resize_origin": ("C", "[D3] [D5] (projection-origin API of the old Workspace, no mount)"),
    },
    "tests/mount_failure.rs": {
        "mount_failure_deadline": ("A+C", "INIT deadline [D5]. Failed attach custody: D `native_mount.rs::attach_refusals_have_no_effect_and_a_failed_attach_returns_to_unattached`, D `native_mount_routes.rs::lost_bind_and_attach_acknowledgements_are_located_without_replay`"),
        "mount_failure_worker": ("A", "Fixed shared workers: F `dispatch.rs::panic_and_failure_retain_original_future_and_do_not_stop_other_mounts`, `handoff_after_pool_shutdown_retains_original_and_returns_terminal_error`; receive-loop spawn failure is proven in the vendored fuser lifecycle tests ([R2 fuser checkpoint](../../R2-FUSER-LIFECYCLE-20261008.md))"),
        "mount_failure_session": ("A", "D `native_mount.rs::attach_refusals_have_no_effect_and_a_failed_attach_returns_to_unattached`"),
        "mount_failure_admission": ("A+C", "Deadline half [D5]. Authority half: D `native_mount.rs::attach_refusals_have_no_effect_and_a_failed_attach_returns_to_unattached`"),
        "mount_failure_success": ("A+C", "Read-only mount profile [D13]. Writable lifecycle: D `native_control.rs::native_mount_commit_status_fork_history_and_terminal_unmount`"),
    },
}

FUSE_OTHER = {
    "Cargo.toml": ("C", "Package name `layerfs-fuse` duplicates the active member; depends on the replaced Workspace API; cannot be loaded ([receipt 02](02-cargo-cannot-load.txt))"),
    "tests/check_mount.py": ("C", "Docker driver of the old mount tests"),
    "tests/kernel_resize_route.py": ("C", "Docker driver of `kernel_resize.rs`"),
    "tests/kernel_write_route.py": ("C", "Docker driver of `kernel_write.rs`"),
    "tests/mount_failure_route.py": ("C", "Docker driver of `mount_failure.rs`"),
    "tests/position_manifest.py": ("C", "Fixture manifest for the old position tests"),
    "tests/position_master.py": ("C", "Fixture master for the old position tests"),
}

SANDBOX_SRC = {
    "src/lib.rs": ("A+C", "Exports `SandboxOwner`, routes and `LogCapture`", "`layerfs-sandbox/src/lib.rs`, SDK `sandbox/api.rs`"),
    "src/docker.rs": ("A+C", "Docker CLI invocation: create, port lookup, delete, bounded log capture at delete", "Engine API client `layerfs-sandbox/src/backend/docker/` (S `engine_protocol.rs` 9, S `engine_lifecycle.rs` 6). Delete-time log capture is not in the selected SandboxApi scope [D11]"),
    "src/owner.rs": ("A+C", "Host registry of sandboxes: create, list, delete, checked routes and Workspace bindings to the host Server", "Create, start, stop, delete with one-attempt flags: SDK `sandbox/api.rs` + `layerfs-sandbox/src/backend/docker/container.rs` (S `engine_lifecycle.rs::partial_create_identity_and_nonlocal_volume_are_not_adopted`, `lost_upload_and_start_keep_original_owner_and_refuse_replay`). The registry routed Workspace calls to the host Server and is retired with it [D1]; `list` is not in the selected scope [D11]"),
    "src/readiness.rs": ("A", "Wait for the daemon's readiness marker", "`layerfs-sandbox/src/backend/docker/listener.rs`: S `engine_lifecycle.rs::stderr_and_substring_markers_never_satisfy_readiness`, `marker_inside_truncated_frame_keeps_evidence_and_fails_readiness`"),
    "src/session.rs": ("A+C", "Bounded reuse of one authenticated Workspace control session", "SDK `control/connection.rs` (K `observed_control.rs` 3 tests; D `native_control.rs::control_busy_keeps_the_channel_healthy_for_a_later_explicit_call`). Session reuse across the host registry [D1]"),
}

SANDBOX_TESTS = {
    "tests/docker_port_pending.rs": {
        "docker_allocates_port_and_pending_record_cannot_route": ("A+C", "Pending registry record [D1]. Endpoint from the Engine: S `engine_lifecycle.rs::streamed_private_archive_exact_marker_endpoint_and_borrowed_cleanup`, `widened_or_missing_native_access_is_never_reported_as_the_endpoint`"),
    },
}

SANDBOX_OTHER = {
    "Cargo.toml": ("C", "Depends on `../layerfs-api/core`, removed by F12; cannot be loaded ([receipt 02](02-cargo-cannot-load.txt))"),
}
