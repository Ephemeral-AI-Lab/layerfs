mod support;

#[cfg(feature = "operation-polymorphism")]
mod l1_content {
    use crate::support;

    use layerfs_storage::qualification::content::semantic::{observe_failure_v1, ContentRequestV1};
    use layerfs_storage::CoreError;

    #[test]
    fn truncation_trailing_input_and_sink_refusal_abort_private_state() {
        for (data, declared, expected) in [
            (&b"short"[..], 6, CoreError::Truncated),
            (&b"trailing"[..], 7, CoreError::TrailingBytes),
        ] {
            let observation = observe_failure_v1(
                &ContentRequestV1::new(b"file", 0o600, data).with_declared_len(declared),
            );
            assert_eq!(observation.error(), expected);
            assert!(!observation.sink_completed());
            assert_eq!(observation.sink_aborts(), 1);
            assert!(observation.spool_aborted());
            assert!(!observation.sink_active());
            assert_eq!(observation.admitted_slots(), 0);
        }

        let data = support::fastcdc_golden_input(40_000);
        let observation = observe_failure_v1(
            &ContentRequestV1::new(b"file", 0o600, &data).with_sink_refusal_after(60),
        );
        assert_eq!(observation.error(), CoreError::SinkRefused);
        assert!(!observation.sink_completed());
        assert_eq!(observation.sink_aborts(), 1);
        assert!(observation.spool_aborted());
        assert!(!observation.sink_active());
        assert_eq!(observation.admitted_slots(), 0);
    }
}

#[cfg(feature = "operation-polymorphism")]
mod l1_update {
    use crate::support;
    use crate::support::counting_source::CountingSource;

    use layerfs_storage::qualification::content::semantic::{
        base_budget_bytes, create_v1, expected_planned_high_water, first_chunk_end,
        max_update_resynchronization_bytes, update_from_reader_v1, update_v1, ContentRequestV1,
        UpdateRequestV1,
    };
    use layerfs_storage::CoreError;

    fn edited(base: &[u8], start: usize, end: usize, inserted: &[u8]) -> Vec<u8> {
        let mut result = Vec::with_capacity(base.len() - (end - start) + inserted.len());
        result.extend_from_slice(&base[..start]);
        result.extend_from_slice(inserted);
        result.extend_from_slice(&base[end..]);
        result
    }

    #[test]
    fn insertion_deletion_replacement_and_edge_ranges_match_full_canonical_results() {
        let base = support::fastcdc_golden_input(220_000);
        let first_chunk_end = first_chunk_end(&base).unwrap();
        let cases = [
            (50_000, 50_000, b"inserted bytes".as_slice()),
            (60_000, 75_000, b"".as_slice()),
            (31_000, 31_018, b"same length bytes!".as_slice()),
            (0, first_chunk_end, b"".as_slice()),
            (210_000, 220_000, b"tail replacement".as_slice()),
        ];
        for (start, end, insertion) in cases {
            let observation = update_v1(&UpdateRequestV1::new(&base, start, end, insertion));
            let expected_data = edited(&base, start as usize, end as usize, insertion);
            let expected = create_v1(&ContentRequestV1::new(
                b"expected.bin",
                0o644,
                &expected_data,
            ))
            .expect("independent complete Create result");
            assert_eq!(observation.error(), None, "range {start}..{end}");
            assert_eq!(observation.logical_id(), expected.logical_id());
            assert_eq!(observation.physical_id(), expected.physical_id());
            assert!(observation.sink_completed());
            assert_eq!(
                observation.output_ref_count(),
                observation.prepared_chunk_count(),
                "range {start}..{end}"
            );
            assert!(observation.sink_object_count() <= observation.prepared_chunk_count() + 1);
            assert!(observation.base_bytes_read() <= 32_768 + max_update_resynchronization_bytes());
            assert!(
                observation.update_resynchronization_bytes()
                    <= max_update_resynchronization_bytes()
            );
            assert_eq!(observation.automatic_fallbacks(), 0);
            assert_eq!(observation.redispatches(), 0);
            assert_eq!(observation.publication_authority_dispatches(), 0);
            assert_eq!(observation.admitted_slots(), 0);
            let output_len = base.len() as u64 - (end - start) + insertion.len() as u64;
            assert_eq!(
                observation.planned_memory_high_water(),
                expected_planned_high_water(output_len)
            );
        }
    }

    #[test]
    fn middle_update_reuses_authenticated_prefix_and_suffix_identities() {
        let base = support::fastcdc_golden_input(300_000);
        let request = UpdateRequestV1::new(&base, 120_000, 120_010, b"changed");
        let mut source = CountingSource::new(b"changed");
        let observation = update_from_reader_v1(&request, 7, &mut source);
        assert_eq!(observation.error(), None);
        assert_eq!(source.bytes_read(), 7);
        assert_eq!(source.reads(), 2);
        assert!(observation.logical_chunks_reused() >= 2);
        assert!(observation.bytes_structurally_reused() > 0);
        assert!(observation.anchor_attempts() > 0);
        assert!(observation.sink_object_count() < observation.output_ref_count() + 1);
        assert_eq!(observation.automatic_fallbacks(), 0);
        assert_eq!(observation.admitted_slots(), 0);
    }

    #[test]
    fn rejoin_requires_a_complete_exact_base_byte_comparison() {
        let base = support::fastcdc_golden_input(300_000);
        let request = UpdateRequestV1::new(&base, 120_000, 120_010, b"changed");
        let successful = update_v1(&request);
        assert_eq!(successful.error(), None);
        assert!(successful.base_read_calls() >= 2);
        assert_eq!(successful.admitted_slots(), 0);

        let failed = update_v1(&request.with_base_failure_on(successful.base_read_calls()));
        assert_eq!(failed.error(), Some(CoreError::RangeResyncFailed));
        assert!(!failed.sink_completed());
        assert_eq!(failed.sink_aborts(), 1);
        assert!(failed.output_aborted());
        assert_eq!(failed.automatic_fallbacks(), 0);
        assert_eq!(failed.admitted_slots(), 0);
    }

    #[test]
    fn missing_or_mismatched_evidence_and_no_anchor_fail_closed() {
        let base = support::fastcdc_golden_input(300_000);
        let request = UpdateRequestV1::new(&base, 1_000, 1_010, b"x");

        let missing = update_v1(&request.with_missing_evidence(true));
        assert_eq!(missing.error(), Some(CoreError::RangeResyncFailed));
        assert_eq!(missing.base_bytes_read(), 0);
        assert_eq!(missing.inserted_reads(), 0);
        assert_eq!(missing.update_failures(), 1);
        assert_eq!(missing.admitted_slots(), 0);

        let logical_mismatch = update_v1(&request.with_mismatched_logical(true));
        assert_eq!(logical_mismatch.error(), Some(CoreError::RangeResyncFailed));
        assert_eq!(logical_mismatch.base_bytes_read(), 0);
        assert_eq!(logical_mismatch.inserted_reads(), 0);
        assert_eq!(logical_mismatch.admitted_slots(), 0);

        let physical_mismatch = update_v1(&request.with_mismatched_physical(true));
        assert_eq!(
            physical_mismatch.error(),
            Some(CoreError::RangeResyncFailed)
        );
        assert_eq!(physical_mismatch.base_bytes_read(), 0);
        assert_eq!(physical_mismatch.inserted_reads(), 0);
        assert_eq!(physical_mismatch.admitted_slots(), 0);

        let failed = update_v1(&request.with_no_anchor(true));
        assert_eq!(failed.error(), Some(CoreError::RangeResyncFailed));
        assert!(failed.update_resynchronization_bytes() > 0);
        assert!(failed.update_resynchronization_bytes() <= max_update_resynchronization_bytes());
        assert!(failed.base_bytes_read() < base.len() as u64);
        assert_eq!(failed.automatic_fallbacks(), 0);
        assert!(!failed.sink_completed());
        assert_eq!(failed.sink_aborts(), 1);
        assert!(failed.output_aborted());
        assert_eq!(failed.admitted_slots(), 0);
    }

    #[test]
    fn invalid_range_and_resource_refusal_precede_all_reads() {
        let base = support::fastcdc_golden_input(40_000);
        let reversed = update_v1(&UpdateRequestV1::new(&base, 11, 10, b"x"));
        assert_eq!(reversed.error(), Some(CoreError::RangeResyncFailed));
        assert_eq!(reversed.base_bytes_read(), 0);
        assert_eq!(reversed.inserted_reads(), 0);
        assert_eq!(reversed.admitted_slots(), 0);
        let past_end = update_v1(&UpdateRequestV1::new(&base, 0, 40_001, b"x"));
        assert_eq!(past_end.error(), Some(CoreError::RangeResyncFailed));
        assert_eq!(past_end.base_bytes_read(), 0);
        assert_eq!(past_end.inserted_reads(), 0);
        assert_eq!(past_end.admitted_slots(), 0);

        let observation =
            update_v1(&UpdateRequestV1::new(&base, 10, 20, b"x").with_budget(base_budget_bytes()));
        assert_eq!(observation.error(), Some(CoreError::ResourceRefused));
        assert_eq!(observation.base_bytes_read(), 0);
        assert_eq!(observation.inserted_reads(), 0);
        assert_eq!(observation.automatic_fallbacks(), 0);
        assert_eq!(observation.admitted_slots(), 0);

        let oversized_evidence = update_v1(
            &UpdateRequestV1::new(&base, 10, 20, b"x").with_evidence_residency(4 * 1024 * 1024),
        );
        assert_eq!(
            oversized_evidence.error(),
            Some(CoreError::RangeResyncFailed)
        );
        assert_eq!(oversized_evidence.base_bytes_read(), 0);
        assert_eq!(oversized_evidence.inserted_reads(), 0);
        assert_eq!(oversized_evidence.automatic_fallbacks(), 0);
        assert_eq!(oversized_evidence.admitted_slots(), 0);

        let oversized_base = update_v1(
            &UpdateRequestV1::new(&base, 10, 20, b"x").with_base_residency(4 * 1024 * 1024),
        );
        assert_eq!(oversized_base.error(), Some(CoreError::RangeResyncFailed));
        assert_eq!(oversized_base.base_bytes_read(), 0);
        assert_eq!(oversized_base.inserted_reads(), 0);
        assert_eq!(oversized_base.automatic_fallbacks(), 0);
        assert_eq!(oversized_base.admitted_slots(), 0);
    }
}

#[cfg(feature = "operation-polymorphism")]
mod mutation_owner {
    use crate::support::temp_fs_cas::TempFsCas;
    use layerfs_storage::qualification::lifecycle::semantic::{
        complete_mutation_case_v1, CompleteMutationCaseV1, CompleteMutationCountersV1,
        CompleteMutationObservationV1, CompleteMutationTerminalV1,
    };

    fn observe(
        label: &str,
        case: CompleteMutationCaseV1,
        update_base: &[u8],
    ) -> CompleteMutationObservationV1 {
        let root = TempFsCas::new(label);
        root.create_dir();
        complete_mutation_case_v1(root.path(), case, update_base)
    }

    fn assert_clean(observation: CompleteMutationObservationV1) {
        assert_eq!(observation.operation_admitted_slots, 0);
        assert_eq!(observation.operation_admission_active, 0);
        assert_eq!(observation.storage_admission_active, (0, 0, 0));
        assert_eq!(observation.preparation_entries, 0);
        assert!(observation.authority_clean);
        assert!(observation.namespace_entries_are_regular);
        assert!(observation.root_usable);
        assert!(observation.stale_usable);
        assert!(observation.counters.zero_forbidden_work);
        assert!(observation.counters.storage_equations_hold);
    }

    fn assert_storage_terminal(counters: CompleteMutationCountersV1) {
        assert!(counters.storage_bytes_requested > 0);
        assert_eq!(
            counters.storage_bytes_requested,
            counters.storage_bytes_reserved
        );
        assert_eq!(
            counters.storage_bytes_reserved,
            counters.storage_bytes_released
                + counters.storage_bytes_committed
                + counters.storage_bytes_retained
        );
        assert_eq!(
            counters.storage_inodes_requested,
            counters.storage_inodes_reserved
        );
        assert_eq!(
            counters.storage_inodes_reserved,
            counters.storage_inodes_released
                + counters.storage_inodes_committed
                + counters.storage_inodes_retained
        );
        assert!(
            counters.root_storage_active_reserved_bytes_lifetime_high_water
                >= counters.storage_bytes_reserved
        );
        assert!(
            counters.root_storage_active_reserved_inodes_lifetime_high_water
                >= counters.storage_inodes_reserved
        );
        assert!(counters.storage_bytes_committed > 0);
        assert!(counters.storage_inodes_committed > 0);
        assert_eq!(counters.storage_bytes_retained, 0);
        assert_eq!(counters.storage_inodes_retained, 0);
        assert_eq!(counters.mutable_preparation_residue_bytes, 0);
        assert_eq!(counters.mutable_preparation_residue_inodes, 0);
        assert_eq!(counters.unreachable_installed_residue_bytes, 0);
        assert!(counters.zero_forbidden_work);
    }

    #[test]
    fn complete_replace_and_metadata_reach_independently_derived_handoffs() {
        let observation = observe(
            "complete-replace-metadata",
            CompleteMutationCaseV1::ReplaceAndMetadata,
            &[],
        );
        assert_eq!(observation.terminal, CompleteMutationTerminalV1::Succeeded);
        assert_eq!(observation.expected_roots_matched, 3);
        assert_eq!(observation.completed_operations, 2);
        assert_eq!(observation.validated_handoffs, 2);
        assert_eq!(observation.storage_terminals, 2);
        assert_eq!(observation.operation_counter_count, 2);
        assert!(observation.algorithm_is_fastcdc);
        assert_storage_terminal(observation.operation_counters[0]);
        assert_storage_terminal(observation.operation_counters[1]);
        assert_clean(observation);
    }

    #[test]
    fn complete_add_move_and_remove_use_one_candidate_graph_each() {
        let observation = observe(
            "complete-add-move-remove",
            CompleteMutationCaseV1::AddMoveRemove,
            &[],
        );
        assert_eq!(observation.terminal, CompleteMutationTerminalV1::Succeeded);
        assert_eq!(observation.expected_roots_matched, 4);
        assert_eq!(observation.completed_operations, 3);
        assert_eq!(observation.validated_handoffs, 3);
        assert_eq!(observation.storage_terminals, 3);
        assert_eq!(observation.operation_counter_count, 3);
        assert!(observation.final_root_returns_to_base);
        assert!(observation.algorithm_is_fastcdc);
        assert_storage_terminal(observation.operation_counters[0]);
        assert_storage_terminal(observation.operation_counters[1]);
        assert_storage_terminal(observation.operation_counters[2]);
        assert_clean(observation);
    }

    #[test]
    fn complete_cross_directory_move_detaches_and_attaches_in_one_handoff() {
        let observation = observe(
            "complete-cross-directory-move",
            CompleteMutationCaseV1::CrossDirectoryMove,
            &[],
        );
        assert_eq!(observation.terminal, CompleteMutationTerminalV1::Succeeded);
        assert_eq!(observation.expected_roots_matched, 2);
        assert_eq!(observation.completed_operations, 1);
        assert_eq!(observation.validated_handoffs, 1);
        assert_eq!(observation.storage_terminals, 1);
        assert_eq!(observation.operation_counter_count, 1);
        assert!(observation.algorithm_is_fastcdc);
        assert_storage_terminal(observation.operation_counters[0]);
        assert_clean(observation);
    }

    #[test]
    fn complete_update_authenticates_and_rejoins_without_replace_fallback() {
        let base = crate::support::fastcdc_golden_input(300_000);
        let observation = observe("complete-update", CompleteMutationCaseV1::Update, &base);
        assert_eq!(observation.terminal, CompleteMutationTerminalV1::Succeeded);
        assert_eq!(observation.expected_roots_matched, 2);
        assert_eq!(observation.completed_operations, 1);
        assert_eq!(observation.validated_handoffs, 1);
        assert_eq!(observation.storage_terminals, 1);
        assert_eq!(observation.source_offset, 7);
        assert_eq!(observation.operation_counter_count, 1);
        assert!(observation.algorithm_is_fastcdc);
        assert!(observation.counters.update_resynchronization_bytes > 0);
        assert!(observation.counters.anchor_attempts > 0);
        assert_storage_terminal(observation.operation_counters[0]);
        assert_clean(observation);
    }

    #[test]
    fn complete_update_reference_metadata_overflow_is_transactional_and_terminal() {
        let base = crate::support::fastcdc_golden_input(300_000);
        let observation = observe(
            "complete-update-reference-overflow",
            CompleteMutationCaseV1::UpdateReferenceMetadataOverflow,
            &base,
        );
        let counters = observation.counters;
        assert_eq!(
            observation.terminal,
            CompleteMutationTerminalV1::IntegerOverflow
        );
        assert_eq!(observation.expected_roots_matched, 1);
        assert_eq!(observation.completed_operations, 0);
        assert_eq!(observation.validated_handoffs, 0);
        assert_eq!(observation.source_offset, 0);
        assert_eq!(counters.update_reference_metadata_records, 7);
        assert_eq!(counters.update_reference_metadata_bytes, u64::MAX);
        assert_eq!(counters.update_base_payload_bytes, 0);
        assert_eq!(counters.update_inserted_bytes, 0);
        assert_eq!(counters.update_resynchronization_bytes, 0);
        assert_eq!(counters.exact_rejoin_bytes, 0);
        assert_eq!(counters.anchor_attempts, 0);
        assert_eq!(counters.source_read_calls, 0);
        assert_eq!(counters.source_bytes_read, 0);
        assert_eq!(counters.fscas_bytes_read, 356);
        assert_eq!(counters.fscas_read_calls, 17);
        assert_eq!(
            counters.storage_bytes_requested,
            counters.storage_bytes_reserved
        );
        assert_eq!(
            counters.storage_bytes_reserved,
            counters.storage_bytes_released
                + counters.storage_bytes_committed
                + counters.storage_bytes_retained
        );
        assert_eq!(
            counters.storage_inodes_requested,
            counters.storage_inodes_reserved
        );
        assert_eq!(
            counters.storage_inodes_reserved,
            counters.storage_inodes_released
                + counters.storage_inodes_committed
                + counters.storage_inodes_retained
        );
        assert_eq!(counters.storage_bytes_committed, 0);
        assert_eq!(counters.storage_inodes_committed, 0);
        assert_eq!(counters.storage_bytes_retained, 0);
        assert_eq!(counters.storage_inodes_retained, 0);
        assert_eq!(counters.unreachable_installed_residue_bytes, 0);
        assert_eq!(counters.mutable_preparation_residue_bytes, 0);
        assert_eq!(counters.mutable_preparation_residue_inodes, 0);
        assert!(observation.namespace_unchanged);
        assert_clean(observation);
    }

    #[test]
    fn complete_update_exact_rejoin_overflow_is_transactional_and_terminal() {
        let base = crate::support::fastcdc_golden_input(300_000);
        let observation = observe(
            "complete-update-rejoin-overflow",
            CompleteMutationCaseV1::UpdateExactRejoinOverflow,
            &base,
        );
        let counters = observation.counters;
        assert_eq!(
            observation.terminal,
            CompleteMutationTerminalV1::IntegerOverflow
        );
        assert_eq!(observation.expected_roots_matched, 1);
        assert_eq!(observation.completed_operations, 0);
        assert_eq!(observation.validated_handoffs, 0);
        assert_eq!(observation.source_offset, 7);
        assert_eq!(counters.exact_rejoin_bytes, 7);
        assert_eq!(counters.rejoin_successes, u64::MAX);
        assert_eq!(counters.rejoin_failures, 11);
        assert_eq!(counters.bytes_read, 74_342);
        assert_eq!(counters.source_read_calls, 2);
        assert_eq!(counters.source_bytes_read, 7);
        assert_eq!(counters.update_base_payload_bytes, 73_979);
        assert_eq!(counters.update_inserted_bytes, 7);
        assert_eq!(counters.update_reference_metadata_records, 28);
        assert_eq!(counters.update_reference_metadata_bytes, 1_008);
        assert_eq!(counters.update_resynchronization_bytes, 67_808);
        assert_eq!(counters.anchor_attempts, 1);
        assert_eq!(counters.fscas_bytes_read, 356);
        assert_eq!(counters.fscas_read_calls, 17);
        assert_eq!(counters.update_failures, 1);
        assert_eq!(
            counters.storage_bytes_requested,
            counters.storage_bytes_reserved
        );
        assert_eq!(
            counters.storage_bytes_reserved,
            counters.storage_bytes_released
                + counters.storage_bytes_committed
                + counters.storage_bytes_retained
        );
        assert_eq!(
            counters.storage_inodes_requested,
            counters.storage_inodes_reserved
        );
        assert_eq!(
            counters.storage_inodes_reserved,
            counters.storage_inodes_released
                + counters.storage_inodes_committed
                + counters.storage_inodes_retained
        );
        assert_eq!(counters.storage_bytes_committed, 0);
        assert_eq!(counters.storage_inodes_committed, 0);
        assert_eq!(counters.storage_bytes_retained, 0);
        assert_eq!(counters.storage_inodes_retained, 0);
        assert_eq!(counters.unreachable_installed_residue_bytes, 0);
        assert_eq!(counters.mutable_preparation_residue_bytes, 0);
        assert_eq!(counters.mutable_preparation_residue_inodes, 0);
        assert_eq!(counters.storage_preparation_bytes_current_after_cleanup, 0);
        assert_eq!(counters.storage_preparation_inodes_current_after_cleanup, 0);
        assert!(observation.namespace_unchanged);
        assert_clean(observation);
    }

    #[test]
    fn complete_mutation_rejects_an_unauthenticated_base_without_preparation() {
        let observation = observe(
            "complete-unauthenticated-base",
            CompleteMutationCaseV1::UnauthenticatedBase,
            &[],
        );
        assert_eq!(
            observation.terminal,
            CompleteMutationTerminalV1::UnauthenticatedBase
        );
        assert_eq!(observation.expected_roots_matched, 1);
        assert_eq!(observation.completed_operations, 0);
        assert_eq!(observation.validated_handoffs, 0);
        assert_eq!(observation.source_offset, 0);
        assert!(observation.namespace_unchanged);
        assert!(observation.accepted_version_differs);
        assert_clean(observation);
    }
}
