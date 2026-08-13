mod support;

mod l1_content {
    use crate::support;

    use layerfs_storage::content::semantic::{observe_failure_v1, ContentRequestV1};
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
        assert_eq!(observation.sink_aborts(), 1);
        assert!(observation.spool_aborted());
        assert!(!observation.sink_active());
        assert_eq!(observation.admitted_slots(), 0);
    }
}

mod l1_update {
    use crate::support;

    use layerfs_storage::content::semantic::{
        base_budget_bytes, expected_planned_high_water, first_chunk_end,
        max_update_resynchronization_bytes, update_v1, UpdateRequestV1,
    };
    use layerfs_storage::CoreError;

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
            assert_eq!(observation.error(), None, "range {start}..{end}");
            assert_ne!(observation.logical_id(), [0; 32]);
            assert_ne!(observation.physical_id(), [0; 32]);
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
        let observation = update_v1(&UpdateRequestV1::new(&base, 120_000, 120_010, b"changed"));
        assert_eq!(observation.error(), None);
        assert!(observation.logical_chunks_reused() >= 2);
        assert!(observation.bytes_structurally_reused() > 0);
        assert!(observation.anchor_attempts() > 0);
        assert!(observation.sink_object_count() < observation.output_ref_count() + 1);
        assert_eq!(observation.automatic_fallbacks(), 0);
    }

    #[test]
    fn rejoin_requires_a_complete_exact_base_byte_comparison() {
        let base = support::fastcdc_golden_input(300_000);
        let request = UpdateRequestV1::new(&base, 120_000, 120_010, b"changed");
        let successful = update_v1(&request);
        assert_eq!(successful.error(), None);
        assert!(successful.base_read_calls() >= 2);

        let failed = update_v1(&request.with_base_failure_on(successful.base_read_calls()));
        assert_eq!(failed.error(), Some(CoreError::RangeResyncFailed));
        assert!(!failed.sink_completed());
        assert_eq!(failed.sink_aborts(), 1);
        assert!(failed.output_aborted());
        assert_eq!(failed.automatic_fallbacks(), 0);
    }

    #[test]
    fn missing_or_mismatched_evidence_and_no_anchor_fail_closed() {
        let base = support::fastcdc_golden_input(300_000);
        let request = UpdateRequestV1::new(&base, 1_000, 1_010, b"x");

        for failed in [
            update_v1(&request.with_missing_evidence(true)),
            update_v1(&request.with_mismatched_logical(true)),
            update_v1(&request.with_mismatched_physical(true)),
        ] {
            assert_eq!(failed.error(), Some(CoreError::RangeResyncFailed));
            assert_eq!(failed.base_bytes_read(), 0);
            assert_eq!(failed.inserted_reads(), 0);
            assert!(failed.sink_aborts() <= 1);
            assert!(!failed.sink_completed());
        }

        let failed = update_v1(&request.with_no_anchor(true));
        assert_eq!(failed.error(), Some(CoreError::RangeResyncFailed));
        assert!(failed.update_resynchronization_bytes() > 0);
        assert!(
            failed.update_resynchronization_bytes()
                <= max_update_resynchronization_bytes()
        );
        assert!(failed.base_bytes_read() < base.len() as u64);
        assert_eq!(failed.automatic_fallbacks(), 0);
        assert_eq!(failed.sink_aborts(), 1);
        assert!(failed.output_aborted());
    }

    #[test]
    fn invalid_range_and_resource_refusal_precede_all_reads() {
        let base = support::fastcdc_golden_input(40_000);
        assert_eq!(
            update_v1(&UpdateRequestV1::new(&base, 11, 10, b"x")).error(),
            Some(CoreError::RangeResyncFailed)
        );
        assert_eq!(
            update_v1(&UpdateRequestV1::new(&base, 0, 40_001, b"x")).error(),
            Some(CoreError::RangeResyncFailed)
        );

        let observation = update_v1(
            &UpdateRequestV1::new(&base, 10, 20, b"x").with_budget(base_budget_bytes()),
        );
        assert_eq!(observation.error(), Some(CoreError::ResourceRefused));
        assert_eq!(observation.base_bytes_read(), 0);
        assert_eq!(observation.inserted_reads(), 0);
        assert_eq!(observation.automatic_fallbacks(), 0);

        for request in [
            UpdateRequestV1::new(&base, 10, 20, b"x").with_evidence_residency(4 * 1024 * 1024),
            UpdateRequestV1::new(&base, 10, 20, b"x").with_base_residency(4 * 1024 * 1024),
        ] {
            let observation = update_v1(&request);
            assert_eq!(observation.error(), Some(CoreError::RangeResyncFailed));
            assert_eq!(observation.base_bytes_read(), 0);
            assert_eq!(observation.inserted_reads(), 0);
            assert_eq!(observation.automatic_fallbacks(), 0);
        }
    }
}

#[cfg(feature = "operation-polymorphism")]
mod mutation_owner {
    use crate::support::counting_source::CountingSource;

    use layerfs_storage::content::semantic::{
        max_update_resynchronization_bytes, update_v1, UpdateRequestV1,
    };
    use layerfs_storage::cow::semantic::{
        file_replacement_v1, mutate_v1, TreeMutationFaultV1, TreeMutationRequestV1,
    };
    use layerfs_storage::CoreError;

    fn assert_successful_tree(
        observation: layerfs_storage::cow::semantic::TreeMutationObservationV1,
        expected_entries: u32,
    ) {
        assert_eq!(observation.error(), None);
        assert_eq!(observation.result_entries(), expected_entries);
        assert!(observation.emitted_objects() > 0);
        assert!(observation.tree_nodes_created() > 0);
        assert!(observation.sink_objects() > 0);
        assert!(observation.sink_begins() > 0);
        assert!(observation.created_bytes() > 0);
        assert!(observation.affected_entries() > 0);
        assert!(observation.logical_matches_rebuild());
        assert!(observation.physical_matches_rebuild());
        assert!(observation.created_objects_are_tree_pages());
    }

    #[test]
    fn complete_replace_and_metadata_reach_independently_derived_handoffs() {
        let observation = file_replacement_v1(17).expect("bounded replacement");
        assert_successful_tree(observation, 300);
        assert_eq!(observation.changed_leaf_index(), 0);
        assert!(observation.changed_leaf_id_differs());
        assert!(observation.unchanged_child_reused());
        assert!(observation.unchanged_leaf_reused() || observation.reused_leaves() > 0);
        assert!(observation.tree_nodes_reused() > 0);
        assert!(observation.proof_window_bytes() > 0);
        assert!(observation.proof_node_count() > 0);
        assert!(observation.page_depth() > 0);
    }

    #[test]
    fn complete_add_move_and_remove_use_one_candidate_graph_each() {
        let added = mutate_v1(TreeMutationRequestV1::add(9_000, 9_000)).expect("bounded add");
        assert_successful_tree(added, 9_001);
        assert!(added.unchanged_leaf_reused());
        assert!(added.reused_leaves() > 0);
        assert!(added.result_reads() > 0);
        assert_eq!(added.error(), None);

        let removed = mutate_v1(TreeMutationRequestV1::remove(9_001, 9_000)).expect("bounded remove");
        assert_successful_tree(removed, 9_000);
        assert!(removed.result_reads() > 0);
        assert!(removed.base_reads() > 0);
        assert_eq!(removed.error(), None);

        let replaced = mutate_v1(TreeMutationRequestV1::replace(300, 17)).expect("bounded replace");
        assert_successful_tree(replaced, 300);
        assert!(replaced.changed_leaf_id_differs());
        assert_eq!(replaced.result_entries(), 300);
    }

    #[test]
    fn complete_cross_directory_move_detaches_and_attaches_in_one_handoff() {
        let detached = mutate_v1(TreeMutationRequestV1::remove(64, 8)).expect("bounded detach");
        let attached = mutate_v1(TreeMutationRequestV1::add(63, 0)).expect("bounded attach");
        assert_successful_tree(detached, 63);
        assert_successful_tree(attached, 64);
        assert!(detached.logical_matches_rebuild());
        assert!(detached.physical_matches_rebuild());
        assert!(attached.logical_matches_rebuild());
        assert!(attached.physical_matches_rebuild());
        assert!(detached.emitted_objects() > 0);
        assert!(attached.emitted_objects() > 0);
        assert!(detached.created_objects_are_tree_pages());
        assert!(attached.created_objects_are_tree_pages());
    }

    #[test]
    fn complete_update_authenticates_and_rejoins_without_replace_fallback() {
        let base = crate::support::fastcdc_golden_input(300_000);
        let mut source = CountingSource::new(b"changed");
        let mut inserted = [0_u8; 7];
        let copied = source.read(&mut inserted);
        assert_eq!(copied, inserted.len());
        assert_eq!(source.reads(), 1);
        assert_eq!(source.bytes_read(), copied as u64);
        let observation = update_v1(&UpdateRequestV1::new(
            &base,
            120_000,
            120_010,
            &inserted[..copied],
        ));
        assert_eq!(observation.error(), None);
        assert_ne!(observation.logical_id(), [0; 32]);
        assert_ne!(observation.physical_id(), [0; 32]);
        assert!(observation.sink_completed());
        assert_eq!(observation.sink_aborts(), 0);
        assert_eq!(observation.output_aborted(), false);
        assert!(observation.logical_chunks_reused() > 0);
        assert!(observation.bytes_structurally_reused() > 0);
        assert!(observation.anchor_attempts() > 0);
        assert_eq!(observation.automatic_fallbacks(), 0);
        assert_eq!(observation.redispatches(), 0);
        assert_eq!(observation.publication_authority_dispatches(), 0);
        assert!(observation.update_resynchronization_bytes() > 0);
        assert!(observation.base_read_calls() >= 2);
        assert!(
            observation.base_bytes_read()
                <= 32_768 + max_update_resynchronization_bytes()
        );
    }

    #[test]
    fn complete_update_reference_metadata_overflow_is_transactional_and_terminal() {
        let error = mutate_v1(
            TreeMutationRequestV1::replace(300, 17).with_fault(TreeMutationFaultV1::IntegerOverflow),
        )
        .expect_err("metadata overflow must fail before admission");
        assert_eq!(error, CoreError::IntegerOverflow);
        assert!(matches!(error, CoreError::IntegerOverflow));
    }

    #[test]
    fn complete_update_exact_rejoin_overflow_is_transactional_and_terminal() {
        let error = mutate_v1(
            TreeMutationRequestV1::add(300, 17).with_fault(TreeMutationFaultV1::IntegerOverflow),
        )
        .expect_err("rejoin overflow must fail before admission");
        assert_eq!(error, CoreError::IntegerOverflow);
        assert!(matches!(error, CoreError::IntegerOverflow));
        assert_eq!(error, CoreError::IntegerOverflow);
    }

    #[test]
    fn complete_mutation_rejects_an_unauthenticated_base_without_preparation() {
        let observation = mutate_v1(
            TreeMutationRequestV1::replace(300, 17).with_fault(TreeMutationFaultV1::WrongRoot),
        )
        .expect("bounded wrong-root observation");
        assert_eq!(observation.error(), Some(CoreError::IdMismatch));
        assert_eq!(observation.emitted_objects(), 0);
        assert_eq!(observation.sink_objects(), 0);
        assert_eq!(observation.sink_begins(), 0);
        assert_eq!(observation.created_bytes(), 0);
        assert_eq!(observation.bytes_read(), 0);
        assert_eq!(observation.tree_nodes_created(), 0);
        assert_eq!(observation.tree_nodes_reused(), 0);
        assert!(!observation.logical_matches_rebuild());
        assert!(!observation.physical_matches_rebuild());
        assert!(!observation.changed_leaf_id_differs());
        assert!(!observation.unchanged_child_reused());
        assert!(observation.created_objects_are_tree_pages());
        assert_eq!(observation.result_entries(), 300);
        assert!(observation.page_depth() > 0);
    }
}
