# Coverage audit: `layerfs-server`

> **Status:** Dated checkpoint receipt; coverage audit written before any removal.

The host Server: authorized Service over Content, Storage and History, plus the operator process. Classes and the directions `[D1]`–`[D13]` are defined in [00-scope-and-method.md](00-scope-and-method.md). Test directories: D daemon, W workspace, O overlay, F fuse, B bridge, S sandbox, K SDK, P project, C content, PE persistence.

Verdict: **REMOVE. Retired by owner direction [D1]; every behaviour is class A or C.**

## Source

Production lines from the pinned counter at `cf5bd62ce` ([receipt 01](01-production-loc-head.txt)). Total 3996 in 31 files.

| File | Lines | Behaviour | Class | Covered by / recorded direction |
| --- | ---: | --- | --- | --- |
| `src/bin/layerfs-server.rs` | 9 | Operator process entry of the host Server | C | [D1] |
| `src/host/acceptor.rs` | 178 | Bounded native connection acceptor of the host Server | C | [D1]. The daemon's own control acceptor is `layerfs-daemon/src/application/serve.rs` (D `native_application.rs`) |
| `src/host/assembly.rs` | 216 | One host authority: Store, history, Service, listener and Sandbox owner | C | [D1] |
| `src/host/config.rs` | 124 | Telemetry and history-catalog configuration of the Server | C | [D1]. Daemon configuration is `layerfs-daemon/src/application/config.rs` |
| `src/host/mod.rs` | 8 | Declarations of the host assembly | C | [D1] |
| `src/host/run.rs` | 80 | Operator assembly: configured keys, Store path, one acceptor | C | [D1] |
| `src/host/store.rs` | 49 | Store, history, grant and capacity construction | A+C | Host route [D1]. Store create/open: SDK `project/init.rs` (K `init.rs::init_seals_complete_root_and_first_branch_without_retained_handles`), daemon `bootstrap.rs` (D `native_install.rs::authenticated_install_opens_one_store_and_reads_the_complete_root`) |
| `src/lib.rs` | 5 | Exports of Service and Server | C | [D1] |
| `src/service/error.rs` | 114 | Keep C1/C2 failure distinctions across the Service | A+C | Service wire [D1]. Direct Store failures keep their original cause: D `store_commit.rs::busy_missing_dependency_and_read_failure_leave_no_stage_or_local_data_loss` |
| `src/service/handler.rs` | 257 | Store authority, grants, per-Store writer admission, read bound | A+C | Grants and writer budgets [D1]. Replacement rule is one before-effect Busy and fair readers [D5]: PE `serverless_store.rs::second_process_busy_has_no_effect_and_readers_keep_progressing`, D `installed_store.rs::direct_reads_finish_under_another_process_writer_and_busy_stays_typed`, D `store_read_service.rs` (10 tests) |
| `src/service/init_project.rs` | 60 | Project Init through the Service | A | 303/07 §4.1: Init belongs in `layerfs-project`. P `init_sqlite.rs::selected_profiles_init_100_and_1000_pass_the_full_namespace_oracle`, K `init.rs` (2 tests) |
| `src/service/input.rs` | 31 | Sequential wire input, incomplete until exact EOF | C | [D1] [D9] |
| `src/service/mod.rs` | 8 | Declarations of the authorized Service | C | [D1] |
| `src/service/read/catalog.rs` | 149 | Read-only history requests | A+C | Service route [D1]. History reads: PE `history_lifecycle.rs::history_reads_return_the_recorded_ancestry_and_chain`, PE `history_history_pages.rs` (3 tests); control verbs fork/history: D `native_control.rs::native_mount_commit_status_fork_history_and_terminal_unmount` |
| `src/service/read/content.rs` | 190 | Bounded content reads and inspection variants | A+C | Service route [D1]. Daemon reads the Store directly: W `base.rs::full_root_binding_aliases_metadata_symlinks_and_eof_use_content_apis`, D `installed_store.rs` (3 tests), D `mounted_store_read.rs` (4 tests) |
| `src/service/read/mod.rs` | 2 | Declarations | C | [D1] |
| `src/service/records.rs` | 107 | Service wire record conversion | C | [D1] [D9] |
| `src/service/save/catalog.rs` | 280 | Stage, commit, fork and reservation commands after admission | A+C | Service route [D1]. PE `history_conditional_updates.rs` (6 tests), PE `atomic_publication.rs` (3 tests), PE `history_allocation.rs` (3 tests); Commit publication: D `store_commit.rs` (8 tests) |
| `src/service/save/content.rs` | 265 | One Storage Save per mutation; root delivered after EOF and finish | A+C | Service route [D1]. Save lifecycle: PE `interleaved_saves.rs`, PE `sqlite_transaction_units.rs` (10 tests); `Save::finish` in Commit: D `store_commit.rs::committed_up_to_date_overwrite_and_new_bind_preserve_exact_roots` |
| `src/service/save/file_stream.rs` | 533 | Host-side replay of one final Base/Local/Zero file sequence | A+C | Host-side construction [D1] [D9]. Captured files are normalized in the daemon: W `captured_file_edits.rs` (14 tests), W `captured_namespace_fragments.rs` (9 tests) |
| `src/service/save/file_stream/origin_runs.rs` | 135 | Navigation of replacements resolved from the base | A+C | As `file_stream.rs`. W `captured_file_edits.rs::inherited_gap_and_window_crossing_authenticated_eof_are_zero_without_base_fallback` |
| `src/service/save/filesystem.rs` | 68 | Inode values, directory metadata and fresh declarations under one Save | A+C | Host-side construction [D9]. `CapturedNamespace`: W `captured_namespace.rs` (9 tests), W `captured_namespace_states.rs` (18 tests) |
| `src/service/save/import/batch/mod.rs` | 2 | Declarations | A | see below |
| `src/service/save/import/batch/producer.rs` | 86 | Ordered messages from file constructors to one Save owner | A | `layerfs-project` import with bounded batches into `Save::accept`: P `backed_acquisition.rs` (2 tests), P `namespace_scaling.rs` |
| `src/service/save/import/mod.rs` | 3 | Declarations of the import route | A | see below |
| `src/service/save/import/namespace.rs` | 307 | Namespace initialization and inode role validation | A | P `complete_root.rs` (2 tests), P `init_sqlite.rs` (4 tests), P `init_memory.rs` |
| `src/service/save/import/scan.rs` | 247 | One directory import into a single Save | A | P `complete_root.rs::native_import_keeps_ignored_git_dependencies_outputs_and_exact_link_targets`, P `acquisition_custody.rs` (6 tests) |
| `src/service/save/metadata.rs` | 172 | Fresh portable attribute tree or a patch of an existing one | A+C | Service constructor route [D9]. W `captured_namespace_metadata.rs` (2 tests), C `filesystem_attributes.rs` |
| `src/service/save/mod.rs` | 8 | Declarations | C | [D1] |
| `src/service/save/prepared.rs` | 269 | Prepared namespace stream received into a charged row spool | C | [D9]; its 256 MiB total-upload restriction is a named retirement target [D8] |
| `src/service/save/validation.rs` | 34 | Published roots carry the declared inode roles | A | Content validation on the backed route: W `captured_namespace_perturbed.rs` (4 tests), PE `semantic_admission.rs` (2 tests) |

Lines by class: A 739, A+C 2212, C 1045.

## Tests

54 test functions in 8 files. None can be built at HEAD ([receipt 02](02-cargo-cannot-load.txt)), so none is coverage today; each is classified by the behaviour it asserted.

| Test | Class | Covered by / recorded direction |
| --- | --- | --- |
| **`examples/verify_namespace.rs`** (1) | | |
| `lite_sample_covers_anchors_empty_sizes_and_tree_tail` | A | Same test in `layerfs-project/examples/verify_namespace.rs` |
| **`tests/acceptor_idle.rs`** (2) | | |
| `default_session_limit_includes_two_readers` | C | [D1] (host acceptor) |
| `flag_acceptor_ignores_closed_stdin` | C | [D1] (host acceptor) |
| **`tests/admission.rs`** (5) | | |
| `four_writers_are_admitted_then_refused_and_reads_never_queue_behind_them` | A+C | Writer budget [D1]. Replacement: PE `serverless_store.rs::second_process_busy_has_no_effect_and_readers_keep_progressing` |
| `every_supported_budget_admits_its_writers_and_refuses_the_next` | C | [D1] (configured writer budgets do not exist; one writer, one Busy [D5]) |
| `two_sandboxes_over_one_store_share_its_budget` | A+C | [D1]. Several daemons over one Store: D `shared_processes.rs::overlapping_saves_ordered_overwrites_and_killed_daemon_preserve_the_shared_store` (known failing for want of a precondition; not a regression) |
| `another_store_keeps_its_own_budget` | C | [D1] |
| `a_busy_reader_bound_neither_blocks_writers_nor_exceeds_itself` | A+C | [D1]. D `store_read_service.rs::idle_selection_and_replaced_wakeup_do_not_wait_on_a_busy_reader`, D `store_commit.rs::a_held_producer_does_not_hold_workspace_metadata_or_read_capacity` |
| **`tests/construct_metadata.rs`** (3) | | |
| `constructor_without_history_saves_portable_trees_and_reuses_exact_roots` | A | W `captured_namespace_metadata.rs::a_metadata_patch_keeps_every_other_key_and_its_value_root`, W `captured_namespace_states.rs::a_base_file_with_only_metadata_changes_keeps_its_content_root` |
| `validation_authorization_empty_input_and_deadline_fail_before_save` | C | [D1] grants, [D5] deadline. Portable-grammar refusal: W `namespace.rs::chmod_and_utimens_follow_the_portable_grammar_and_keep_one_mtime` |
| `real_store_admission_failure_is_definite_and_explicit_abort_releases_the_slot` | A+C | [D1]. D `mounted_commit_failures.rs::f1_store_busy_before_save_is_settled_and_a_later_commit_is_exact` |
| **`tests/construct_symlink.rs`** (3) | | |
| `constructor_without_history_preserves_exact_targets_and_canonical_reuse` | A | W `captured_namespace_states.rs::a_fresh_symlink_takes_exactly_its_captured_target`, `a_base_directory_and_symlink_keep_their_content_roots_under_new_metadata` |
| `target_authority_body_and_deadline_refusals_do_not_begin_a_save` | C | [D1] [D5]. Target grammar: W `namespace.rs::unrepresentable_names_targets_and_reservations_are_refused_before_any_job` |
| `occupied_save_refuses_definitely_and_explicit_owner_abort_restores_admission` | A+C | [D1]. D `mounted_commit_failures.rs::f5_store_busy_inside_the_producer_is_settled_and_releases_both_owners` |
| **`tests/direct.rs`** (11) | | |
| `construct_read_edit_inspect_and_reopen` | A | C `file_complete.rs`, C `file_read.rs`, C `edit_single.rs`; reopen: PE `sqlite_publication.rs::canonical_save_read_reuse_and_persisted_reservations_survive_reopen` |
| `ten_mib_final_replacement_saves_and_keeps_the_previous_root` | A | W `captured_file_edits.rs::overlapping_writes_shrink_regrow_and_cross_cell_spans_match_final_bytes`; previous root kept: D `mounted_install.rs::a_later_commit_overwrites_the_branch_head_and_keeps_its_captured_parent` |
| `final_extents_match_the_sealed_reference_root_and_partition` | A+C | Host final-extent stream [D9]. W `captured_namespace_fragments.rs::fragmented_writes_of_a_base_file_give_one_root` |
| `final_file_stream_handles_zero_delete_noop_and_malformed_input` | A+C | [D9]. W `captured_namespace_fragments.rs::a_hole_and_written_zeros_are_the_same_bytes`, W `captured_file_edits.rs::same_byte_overwrite_retains_original_root_with_one_classification` |
| `authenticated_generic_save_accepts_4097_separated_final_runs` | A | No edit-count cap [D8]: W `captured_file_edits.rs::many_edits_use_numbered_records_after_install_and_reads_survive_close` |
| `a_spooled_final_stream_replays_many_runs_with_exact_bytes` | C | [D9] (the spool is the host-side mechanism) |
| `denial_partial_and_invalid_inputs_never_succeed` | C | [D1] (Service grants and wire input) |
| `authenticated_network_after_nonblocking_accept` | A+C | [D1]. Authenticated channel: B `native.rs::authenticated_duplex_records_stream_beyond_one_window_with_fixed_buffers` |
| `inherited_deadline_expires_before_direct_input_or_mutation` | C | [D5] (no deadline on a Store operation) |
| `two_writers_overlap_and_capacity_is_reclaimed_after_reverse_completion` | A | PE `interleaved_saves.rs::scoped_registry_interleaves_pending_reads_finish_abort_and_slot_reuse` |
| `versioned_save_resolves_backward_base_without_changing_v1_or_canonical_identity` | C | [D9] (v1/v2 wire Save). Canonical identity under install: W `captured_file_edits.rs::prepare_after_actual_binding_install_rebinds_only_the_retained_original_root` |
| **`tests/history.rs`** (28) | | |
| `native_directory_import_exceeds_bootstrap_and_reads_bytes` | A | P `complete_root.rs::real_host_profiles_acquire_and_reuse_the_exact_complete_root` |
| `native_directory_import_spills_ordering_without_an_entry_cap` | A | P `backed_acquisition.rs::very_wide_directory_is_ordered_through_many_windows` |
| `native_import_gives_each_file_its_own_content_root` | A | P `backed_acquisition.rs::wide_nested_aliased_root_equals_the_whole_namespace_constructor` |
| `legacy_mask_grants_no_history` | C | [D1] (Service grants) |
| `history_profile_and_opcode_must_agree` | C | [D1] [D9] (wire profile) |
| `empty_namespace_initializes_and_reads_back` | A | PE `history_lifecycle.rs::genesis_is_atomic_and_unique` |
| `native_import_initialization_builds_a_real_namespace` | A | K `init.rs::init_seals_complete_root_and_first_branch_without_retained_handles` |
| `stage_commit_add_layer_and_read_back` | A | PE `history_conditional_updates.rs::exact_stages_publish_in_order_with_their_captured_parents`, PE `history_lifecycle.rs::history_reads_return_the_recorded_ancestry_and_chain` |
| `stale_loser_retains_its_exact_stage` | C | [D7] (overwrite-only; a refused publish leaves no stage). Replacement: PE `atomic_publication.rs::overwrite_and_current_root_up_to_date_each_use_one_transaction` |
| `multiple_no_change_stages_are_up_to_date` | A | PE `history_conditional_updates.rs::a_commit_whose_root_equals_its_base_is_no_changes`, D `product_commit.rs::an_unchanged_workspace_is_up_to_date_and_creates_no_commit` |
| `delayed_discard_token_cannot_consume_a_replacement_stage` | A | PE `history_conditional_updates.rs::an_exact_token_is_the_only_stage_a_discard_removes` |
| `already_published_source_is_up_to_date_before_stale_head_refusal` | A | PE `history_conditional_updates.rs::an_exact_earlier_publication_is_up_to_date_before_a_stale_head` |
| `wrong_role_root_is_refused` | A | PE `history_conditional_updates.rs::stage_insertion_enforces_the_frozen_context`, PE `semantic_admission.rs::omitted_wire_references_cannot_hide_an_inode_dependency_from_save` |
| `rebasing_by_editing_a_token_is_refused` | A | PE `history_conditional_updates.rs::stage_insertion_enforces_the_frozen_context` |
| `inode_reservations_are_scope_wide_and_never_recycled` | A | PE `history_allocation.rs::reservations_are_monotone_half_open_and_per_scope`, PE `history_remediation.rs::concurrent_reservations_never_overlap_and_discard_does_not_refund` |
| `metadata_only_commands_never_touch_the_content_store` | C | [D1] (Service split of content and catalog authority) |
| `history_pages_are_bounded_and_cursors_are_bound_to_their_range` | A | PE `history_history_pages.rs::a_cursor_is_bound_to_its_range_subject_and_content`, `every_bounded_list_paginates_without_gaps_or_repeats` |
| `read_only_reopen_supports_reads_and_refuses_every_mutation` | A | PE `history_reopen.rs::a_read_only_reopen_reads_every_record_and_mutates_nothing` |
| `encoded_record_widths_match_the_catalog_bounds` | C | [D9] (Service wire widths). Current control records: B `control_golden.rs` (5 tests) |
| `empty_and_interleaved_directories_have_real_canonical_pages` | A | C `filesystem_rows.rs`, C `filesystem_read.rs` |
| `branch_root_descriptor_validates_content_scope_profile_and_actual_serial` | A | PE `history_remediation.rs::exact_source_publication_revalidates_immutable_provenance`, PE `history_reopen.rs::an_incompatible_or_foreign_catalog_is_refused` |
| `composite_failure_keeps_acknowledged_stage_distinct_from_absence` | A+C | Retained stage [D7]. Unknown outcome custody: D `store_commit.rs::unknown_history_acknowledgement_retains_original_intent_and_capture` |
| `refreshed_stack_head_reports_typed_stale_base_on_direct_and_native_routes` | C | [D7] (HeadMoved acceptance is historical) |
| `disjoint_file_edits_still_refuse_stale_publication_without_merging` | C | [D7]. Replacement: D `product_commit.rs::a_stale_unchanged_candidate_overwrites_a_moved_head` |
| `complete_read_attributes_match_over_authenticated_transport` | A+C | Service route [D1]. F `attributes.rs::attributes_preserve_signed_fraction_identity_mode_and_link_count` |
| `daemon_control_is_refused_by_service_even_with_every_store_grant` | C | [D1] |
| `portable_metadata_update_saves_one_tree_and_preserves_all_generic_roots` | A | W `captured_namespace_metadata.rs::a_metadata_patch_keeps_every_other_key_and_its_value_root` |
| `metadata_refusals_preserve_admission_and_never_use_legacy_grants` | C | [D1] |
| **`tests/owned_load.rs`** (1) | | |
| `two_native_writers_keep_streaming_heap_bounded_as_input_grows` | A+C | Service route [D1]. P `namespace_scaling.rs::backed_counts_grow_while_resident_buffers_stay_fixed` |

Tests by class: A 24, A+C 11, C 19.

## Other tracked files

| File | Class | Note |
| --- | --- | --- |
| `Cargo.toml` | C | Depends on `../layerfs-api/core`, removed by F12; cannot be loaded ([receipt 02](02-cargo-cannot-load.txt)) |
| `examples/create_store.rs` | A | Store creation: `layerfs-project/examples/benchmark_init.rs`, SDK `ProjectApi::init` |
| `examples/prepare_store.rs` | A | `layerfs-project/examples/benchmark_init.rs`; daemon fixtures in `layerfs-daemon/tests/support/complete_fixture.rs` |
| `examples/verify_namespace.rs` | A | `layerfs-project/examples/verify_namespace.rs` |
| `examples/verify_checkpoint5.rs` | C | Verifier of the withdrawn host-mediated `workspace_write` family [D1]; runnable only at that family's pinned source |
| `examples/verify_shell.rs` | C | Verifier of the withdrawn host-mediated shell-package family [D1] [D2]; runnable only at its pinned source |
| `tests/construct_metadata_route.py` | C | Docker route driver for the Service constructor [D1] |
| `tests/construct_symlink_route.py` | C | Docker route driver for the Service constructor [D1] |
| `tests/exit_failure.py` | C | Server process exit driver [D1] |
| `tests/interoperability.py` | C | Server/daemon interoperability driver [D1] |
| `tests/shutdown.py` | C | Server shutdown driver [D1] |
| `tests/support/file_save.rs` | C | Helper of `direct.rs` and `history.rs` |
