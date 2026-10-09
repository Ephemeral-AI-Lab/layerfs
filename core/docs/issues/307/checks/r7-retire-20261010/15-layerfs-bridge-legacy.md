# Coverage audit: `layerfs-bridge-legacy`

> **Status:** Dated checkpoint receipt; coverage audit written before any removal.

The previous Bridge: logical operation contract, deadlines, prepared construction and the authenticated native channel. Classes and the directions `[D1]`–`[D13]` are defined in [00-scope-and-method.md](00-scope-and-method.md). Test directories: D daemon, W workspace, O overlay, F fuse, B bridge, S sandbox, K SDK, P project, C content, PE persistence.

Verdict: **NOT REMOVED in this stage. It carries pinned read-only view code, which no active crate covers and whose future is the unanswered owner question O-10. Everything else in it is class A or C; the removal is prepared and needs only the owner's answer.**

## Source

Production lines from the pinned counter at `cf5bd62ce` ([receipt 01](01-production-loc-head.txt)). Total 6834 in 32 files.

| File | Lines | Behaviour | Class | Covered by / recorded direction |
| --- | ---: | --- | --- | --- |
| `src/adapters/mod.rs` | 1 | Declarations | A | — |
| `src/adapters/native/client.rs` | 650 | One logical operation at a time with concurrent upload and response | A+C | Logical data operations [D1] [D9]. Control calls: SDK `control/connection.rs` (K `observed_control.rs`); its view calls are O-10 |
| `src/adapters/native/connection.rs` | 449 | Authenticated duplex connection, bounded records, absolute deadlines | A+C | `layerfs-bridge/src/native/`: B `native.rs` (9 tests). Deadlines and the timeout retry loop are not carried [D5] |
| `src/adapters/native/mod.rs` | 18 | Declarations of the native adapter | A | `layerfs-bridge/src/native/` |
| `src/adapters/native/payload.rs` | 154 | Exact-length logical stream adapters | C | [D9] |
| `src/adapters/native/pipe.rs` | 112 | Deadline-aware pipe I/O with cancellation | C | [D5] [D9] (headless pipe delivery) |
| `src/adapters/native/protocol/control.rs` | 178 | Daemon control payloads | A | `layerfs-bridge/src/control/`: B `control_records.rs` (3), B `control_golden.rs` (5) |
| `src/adapters/native/protocol/execution.rs` | 38 | Exec wire payload | C | [D2] |
| `src/adapters/native/protocol/frame.rs` | 89 | Fixed-width local frames | A+C | Logical framing retired by F12 [D9]. Native records: B `native.rs::malformed_record_quarantines_both_directions_before_body_allocation` |
| `src/adapters/native/protocol/history_failure.rs` | 163 | History failure context on the wire | C | [D1] (history no longer crosses the Bridge as a data call; fork/history are control verbs) |
| `src/adapters/native/protocol/metadata.rs` | 945 | Checked binary metadata of every logical operation | A+C | [D9] (303/07 §4.1 names it for removal). Its view-request encodings are O-10 |
| `src/adapters/native/protocol/mod.rs` | 15 | Declarations | C | — |
| `src/adapters/native/protocol/prepared.rs` | 48 | Prepared request header | C | [D9] |
| `src/adapters/native/protocol/response.rs` | 853 | Closed terminal result encoding | A+C | [D9]. Control replies: B `control_records.rs::replies_preserve_scoped_status_conflict_and_known_publication`. Its view results are O-10 |
| `src/adapters/native/protocol/state.rs` | 55 | Input grammar and cumulative work of logical streams | C | [D9] |
| `src/adapters/native/protocol/workspace_commit.rs` | 216 | Commit terminals and writable Status encodings | A+C | B `control_golden.rs::binding_commit_history_and_session_replies_keep_their_bytes`; staged/HeadMoved encodings [D7] |
| `src/adapters/native/protocol/workspace_view.rs` | 199 | View-lease terminal results | O | Pinned read-only SDK view leases. Not covered by any active crate; whether the product keeps them is the unanswered owner question O-10 (303/08 K20, §4.2, §7) |
| `src/adapters/native/server.rs` | 102 | Supplied-handler logical server | C | [D1] [D9] |
| `src/contract/caller.rs` | 13 | Authentication evidence of a caller | A | `layerfs-bridge/src/native/` verified peer: B `native.rs::wrong_static_identity_fails_the_actual_handshake` |
| `src/contract/control.rs` | 158 | Daemon-targeted control requests | A | `layerfs-bridge/src/control/`: B `control_records.rs::commands_are_bounded_exact_and_reject_every_truncated_prefix` |
| `src/contract/execution.rs` | 27 | Bounded daemon Exec request and result | C | [D2] |
| `src/contract/history.rs` | 349 | History query/command surface and typed results | A+C | Wire surface [D1]. Semantics: PE `history_*.rs`; control verbs: B `control_golden.rs::binding_commit_history_and_session_replies_keep_their_bytes` |
| `src/contract/metadata.rs` | 23 | Portable metadata construction requests | C | [D9] |
| `src/contract/mod.rs` | 23 | Declarations | C | — |
| `src/contract/outcome.rs` | 192 | Typed product outcomes separate from transport uncertainty | A+C | SDK `operation.rs` (`OperationCause`, `OperationFailure`): D `native_control.rs::original_lost_control_reply_is_not_replayed_or_settled_by_an_observer`. Its view outcomes are O-10 |
| `src/contract/prepared_stream.rs` | 387 | Prepared namespace as an ordered stream | C | [D9] (303/07 §4.1) |
| `src/contract/request.rs` | 774 | Closed operation profile, budgets and the 4 GiB file constant | A+C | [D9] [D8]. Control profile: B `control_records.rs`. Its view operations are O-10 |
| `src/contract/source.rs` | 29 | Cooperative logical input capability | C | [D9] (303/07 §4.1) |
| `src/contract/workspace_commit.rs` | 207 | Commit report and writable observations | A+C | B `control_records.rs::replies_preserve_scoped_status_conflict_and_known_publication`; stage records [D7] |
| `src/contract/workspace_request.rs` | 158 | Admission checks of the control request family | A+C | B `control_records.rs::native_block_is_additive_and_bounded_fields_are_refused_before_send`. Its view requests are O-10 |
| `src/contract/workspace_view.rs` | 205 | View-lease requests and results | O | Pinned read-only SDK view leases. Not covered by any active crate; whether the product keeps them is the unanswered owner question O-10 (303/08 K20, §4.2, §7) |
| `src/lib.rs` | 4 | Declarations | A | `layerfs-bridge/src/lib.rs` |

Lines by class: A 372, A+C 4882, C 1176, O 404.

## Tests

80 test functions in 25 files. None can be built at HEAD ([receipt 02](02-cargo-cannot-load.txt)), so none is coverage today; each is classified by the behaviour it asserted.

| Test | Class | Covered by / recorded direction |
| --- | --- | --- |
| **`tests/aead_profile.rs`** (1) | | |
| `negotiated_suite_matches_the_compiled_backend` | A | B `native.rs::successful_native_channels_enable_nodelay_on_both_authenticated_sockets` (the channel refuses a build without the required AEAD profile) |
| **`tests/agent_control.rs`** (1) | | |
| `selected_mount_exec_and_identity_round_trip` | A+C | Exec half [D2]. B `control_records.rs::commands_are_bounded_exact_and_reject_every_truncated_prefix` |
| **`tests/attributes.rs`** (2) | | |
| `complete_attributes_roundtrip_and_reject_invalid_fields` | A+C | Wire [D9]. F `attributes.rs::attributes_preserve_signed_fraction_identity_mode_and_link_count` |
| `identity_inspect_queries_roundtrip_and_check_bounds` | C | [D9] (inspect queries of the Service) |
| **`tests/construct_metadata.rs`** (3) | | |
| `constructor_has_exact_wire_shapes_body_limits_and_shared_metadata_grant` | C | [D9] (metadata constructor wire) |
| `constructor_refuses_invalid_portable_fields_profiles_and_response_bodies` | C | [D9] (metadata constructor wire) |
| `native_constructor_correlates_every_echo_and_preserves_unknown_without_replay` | C | [D9] (metadata constructor wire) |
| **`tests/construct_symlink.rs`** (3) | | |
| `constructor_has_exact_bounds_opaque_target_and_shared_content_grant` | C | [D9] (symlink constructor wire) |
| `constructor_rejects_malformed_metadata_and_nonzero_result_body_budget` | C | [D9] (symlink constructor wire) |
| `native_constructor_correlates_saved_length_and_keeps_terminal_loss_unknown_without_replay` | C | [D9] (symlink constructor wire) |
| **`tests/early_refusal.rs`** (1) | | |
| `early_failure_is_delivered_while_the_bounded_upload_closes` | C | [D9] (logical upload) |
| **`tests/file_save_v2.rs`** (1) | | |
| `authenticated_capability_and_v1_v2_metadata_are_distinct` | C | [D9] |
| **`tests/history_delivery.rs`** (1) | | |
| `mutation_result_data_is_rejected_before_any_output` | C | [D1] [D9] |
| **`tests/history_protocol.rs`** (19) | | |
| `every_query_and_command_round_trips` | C | [D1] [D9] (history wire protocol). Semantics: PE `history_*.rs` |
| `native_import_declares_progress_response_budget` | C | [D1] [D9] (history wire protocol). Semantics: PE `history_*.rs` |
| `a_prepared_declaration_is_the_exact_body_it_describes` | C | [D1] [D9] (history wire protocol). Semantics: PE `history_*.rs` |
| `a_prepared_update_wider_than_one_metadata_frame_round_trips` | C | [D1] [D9] (history wire protocol). Semantics: PE `history_*.rs` |
| `profile_and_opcode_must_agree` | C | [D1] [D9] (history wire protocol). Semantics: PE `history_*.rs` |
| `unknown_suboperations_are_refused_before_mutation` | C | [D1] [D9] (history wire protocol). Semantics: PE `history_*.rs` |
| `identity_widths_and_tags_are_checked` | C | [D1] [D9] (history wire protocol). Semantics: PE `history_*.rs` |
| `page_and_count_bounds_are_checked` | C | [D1] [D9] (history wire protocol). Semantics: PE `history_*.rs` |
| `the_retired_pathless_init_tag_is_refused_and_unassigned` | C | [D1] [D9] (history wire protocol). Semantics: PE `history_*.rs` |
| `every_reply_round_trips` | C | [D1] [D9] (history wire protocol). Semantics: PE `history_*.rs` |
| `a_reply_tag_outside_the_closed_union_is_refused` | C | [D1] [D9] (history wire protocol). Semantics: PE `history_*.rs` |
| `failure_codes_round_trip_and_stay_typed` | C | [D1] [D9] (history wire protocol). Semantics: PE `history_*.rs` |
| `permission_bits_are_total_and_legacy_mask_grants_nothing` | C | [D1] [D9] (history wire protocol). Semantics: PE `history_*.rs` |
| `classification_is_exhaustive_and_semantic` | C | [D1] [D9] (history wire protocol). Semantics: PE `history_*.rs` |
| `minimum_and_maximum_record_encodings_match_the_frozen_widths` | C | [D1] [D9] (history wire protocol). Semantics: PE `history_*.rs` |
| `history_page_budget_includes_tags_counts_and_continuation` | C | [D1] [D9] (history wire protocol). Semantics: PE `history_*.rs` |
| `history_failure_context_round_trips_without_changing_legacy_frames` | C | [D1] [D9] (history wire protocol). Semantics: PE `history_*.rs` |
| `invalid_terminal_reservation_and_descriptor_serial_are_refused` | C | [D1] [D9] (history wire protocol). Semantics: PE `history_*.rs` |
| `the_retired_init_and_the_prepared_commands_are_the_only_replaced_encodings` | C | [D1] [D9] (history wire protocol). Semantics: PE `history_*.rs` |
| **`tests/native_connections.rs`** (6) | | |
| `exec_progress_is_authenticated_and_has_no_result_bytes` | C | [D2] |
| `caller_deadline_covers_connect_and_hello` | C | [D5] |
| `repeated_native_connections_authenticate_with_ordinary_tcp` | A | B `native.rs::successful_native_channels_enable_nodelay_on_both_authenticated_sockets` |
| `definite_missing_inspect_keeps_one_authenticated_session` | A+C | Inspect [D9]. D `native_control.rs::control_busy_keeps_the_channel_healthy_for_a_later_explicit_call` |
| `productive_upload_keeps_the_response_wait_alive` | C | [D5] [D9] |
| `missing_native_reply_preserves_read_deadline_but_not_save_custody` | A+C | Deadline [D5]. B `native.rs::terminal_reply_retains_eof_and_original_platform_fence_outcome`, D `native_control.rs::original_lost_control_reply_is_not_replayed_or_settled_by_an_observer` |
| **`tests/pipe_cancel.rs`** (2) | | |
| `cancelled_read_exact_terminates` | C | [D9] (headless pipe) |
| `cancelled_write_all_terminates` | C | [D9] |
| **`tests/pipe_deadline.rs`** (1) | | |
| `a_full_pipe_fails_at_the_deadline_instead_of_blocking` | C | [D5] [D9] |
| **`tests/portable_metadata.rs`** (3) | | |
| `metadata_operation_has_exact_shapes_and_an_independent_store_grant` | C | [D9] (metadata operation wire) |
| `invalid_portable_fields_and_profiles_are_refused_before_delivery` | C | [D9] (metadata operation wire) |
| `native_metadata_reply_must_echo_the_exact_input_and_preserve_unknown_delivery` | C | [D9] (metadata operation wire) |
| **`tests/prepared_stream.rs`** (5) | | |
| `a_declared_stream_round_trips_row_for_row` | C | [D9] (303/07 §4.1 prepared stream) |
| `an_io_error_at_the_trailing_byte_check_is_not_eof` | C | [D9] (303/07 §4.1 prepared stream) |
| `a_stream_that_lies_about_its_rows_is_refused` | C | [D9] (303/07 §4.1 prepared stream) |
| `a_stream_whose_rows_are_out_of_order_is_refused` | C | [D9] (303/07 §4.1 prepared stream) |
| `a_declaration_past_the_charged_bound_is_refused` | C | [D9] (303/07 §4.1 prepared stream) |
| **`tests/protocol.rs`** (6) | | |
| `fixed_framing_rejects_lengths_states_and_truncations` | A+C | [D9]. B `native.rs::malformed_record_quarantines_both_directions_before_body_allocation` |
| `all_operation_metadata_roundtrips_and_caps_are_checked` | C | [D9] |
| `final_file_stream_charges_extent_records_separately_from_file_length` | C | [D9] |
| `terminal_types_and_explicit_continuation_roundtrip` | C | [D9] |
| `remote_budget_only_shortens_the_declared_duration` | C | [D5] |
| `declared_large_input_has_a_size_consistent_frame_and_time_budget` | C | [D5] [D9] |
| **`tests/relay_write.rs`** (1) | | |
| `result_frames_reach_the_consumer_one_write_each` | C | [D9] |
| **`tests/source.rs`** (1) | | |
| `portable_source_handles_dynamic_input_deadline_and_cancellation` | C | [D5] [D9] |
| **`tests/stream_fragmentation.rs`** (4) | | |
| `short_source_reads_form_bounded_body_frames` | A+C | [D9]. B `native.rs::authenticated_duplex_records_stream_beyond_one_window_with_fixed_buffers` |
| `short_logical_writes_form_bounded_result_frames` | A+C | [D9]. B `native.rs::authenticated_duplex_records_stream_beyond_one_window_with_fixed_buffers` |
| `handler_failure_discards_pending_tail_without_drop_flush` | C | [D9] |
| `buffered_output_checks_deadline_before_accepting_another_fragment` | C | [D5] |
| **`tests/upload_batch.rs`** (1) | | |
| `bulk_upload_batches_records_without_changing_frames` | C | [D9] |
| **`tests/workspace_attach.rs`** (3) | | |
| `attach_is_identity_only_bounded_profile_three_mutation` | A | B `control_records.rs::commands_are_bounded_exact_and_reject_every_truncated_prefix` |
| `attach_and_failed_status_preserve_codes_without_widening_lifecycle` | A | B `control_golden.rs::every_refusal_code_keeps_its_bytes` |
| `authenticated_attach_and_status_correlate_without_replay` | A | D `native_mount_routes.rs::lost_bind_and_attach_acknowledgements_are_located_without_replay` |
| **`tests/workspace_close_clean.rs`** (2) | | |
| `lifecycle_bounds_and_shared_outcomes_preserve_distinct_wire_bytes` | A+C | Separate close [D6]. B `control_golden.rs::every_request_tag_keeps_its_bytes` |
| `authenticated_lifecycle_correlates_each_variant_and_identity_without_replay` | A+C | [D6]. D `native_control.rs::original_lost_control_reply_is_not_replayed_or_settled_by_an_observer` |
| **`tests/workspace_commit.rs`** (4) | | |
| `commit_request_has_its_own_budget_and_never_store_authority` | A+C | Time budget [D5]. B `control_records.rs::commands_are_bounded_exact_and_reject_every_truncated_prefix` |
| `commit_terminal_preserves_full_known_and_observed_records_with_bounded_bytes` | A | B `control_records.rs::replies_preserve_scoped_status_conflict_and_known_publication` |
| `writable_status_preserves_submission_and_reachable_partial_commit_state` | A | B `control_golden.rs::status_keeps_its_bytes_at_every_activity_and_native_phase` |
| `authenticated_commit_and_writable_status_correlate_and_never_replay` | A | D `native_control.rs::another_control_channel_observes_running_commit_and_refuses_unmount_before_effect` |
| **`tests/workspace_status.rs`** (3) | | |
| `status_profile_is_bounded_and_has_no_store_permission` | A | B `control_records.rs::commands_are_bounded_exact_and_reject_every_truncated_prefix` |
| `status_reply_preserves_exact_identity_and_refuses_bad_flags_or_closed_counts` | A | B `control_golden.rs::status_keeps_its_bytes_at_every_activity_and_native_phase` |
| `authenticated_status_rejects_mismatched_identity_and_result_data_without_unknown_mutation` | A | K `observed_control.rs::observed_reply_keeps_original_exact_token_validation` |
| **`tests/workspace_unmount.rs`** (4) | | |
| `unmount_is_an_exact_bounded_lifecycle_mutation_without_store_grants` | A | B `control_golden.rs::every_request_tag_keeps_its_bytes` |
| `entered_outcomes_have_exact_identity_bounds_and_a_closed_retained_code_set` | A | B `control_golden.rs::ready_located_and_retained_keep_their_bytes`, B `forced_records.rs` (5) |
| `authenticated_entered_attempts_and_pre_admission_failures_remain_distinct` | A | D `native_custody.rs::an_unmount_that_cannot_revoke_stops_retained_and_later_replies_repeat_it` |
| `delivered_bad_or_lost_unmount_results_are_unknown_and_never_replayed` | A | D `native_control.rs::original_lost_control_reply_is_not_replayed_or_settled_by_an_observer` |
| **`tests/workspace_view.rs`** (2) | | |
| `view_requests_round_trip_and_refuse_malformed_tokens_and_names` | O | Pinned read-only SDK view leases. Not covered by any active crate; whether the product keeps them is the unanswered owner question O-10 (303/08 K20, §4.2, §7) |
| `view_results_round_trip_the_native_codec` | O | Pinned read-only SDK view leases. Not covered by any active crate; whether the product keeps them is the unanswered owner question O-10 (303/08 K20, §4.2, §7) |

Tests by class: A 15, A+C 10, C 53, O 2.

## Other tracked files

| File | Class | Note |
| --- | --- | --- |
| `Cargo.toml` | C | Uses workspace inheritance while excluded; cannot be loaded |
| `examples/fault_peer.rs` | A | Fault peer of the channel tests. B `native.rs` builds its own peers |
| `examples/public_key.rs` | A | Key derivation helper. Daemon private setup: B `daemon_records.rs` |
| `examples/socket_bounds.rs` | A | Socket buffer observation. B `native.rs::authenticated_duplex_records_stream_beyond_one_window_with_fixed_buffers` |
| `tests/fragmented_saved_root.py` | C | Docker driver of the host Save route [D1] |
| `tests/support/` | C | Helpers of the tests above |
| `tests/fixtures/history-wire-a4a144af.hex` | C | Frozen bytes of the retired history wire [D1] [D9] |
